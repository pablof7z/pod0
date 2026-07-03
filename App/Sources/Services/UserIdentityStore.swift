import Foundation
import Observation
import os.log

/// The human user's Nostr identity. The Rust kernel (NMP) owns ALL key
/// material and signing — this store holds NO private keys and performs NO
/// crypto. It is a thin, observable mirror of the kernel identity projection
/// (`KernelModel.kernelIdentity`): it surfaces the active pubkey and mode for
/// the UI, and routes identity actions (import nsec, generate account, connect
/// bunker, sign out) to the kernel.
///
/// Two flavours of identity are supported, both kernel-backed:
/// 1. **Local key** — a private key the kernel generated or imported and
///    persists in its own identity store (`identity.json`).
/// 2. **Remote signer (NIP-46)** — a "bunker" connection where the key lives
///    in a remote signer app; the kernel delegates signing over a relay.
///
/// Identity state arrives reactively: every kernel snapshot tick calls
/// `applyKernelIdentity(...)`, which reconciles `publicKeyHex` / `mode` from
/// the kernel's `activeAccount`. Call `start()` once at launch for legacy
/// keychain cleanup; the kernel restores its own identity asynchronously.
@MainActor
@Observable
final class UserIdentityStore {
    private let logger = Logger.app("UserIdentityStore")

    /// The user's lowercase 64-hex public key, mirrored from
    /// `PodcastUpdate.active_account.pubkey_hex`.
    /// `nil` while no identity is configured.
    private(set) var publicKeyHex: String?

    /// Bech32 `npub1…` encoding of the active public key, mirrored from
    /// `PodcastUpdate.active_account.npub`. Always present when an identity is
    /// loaded; `nil` on fresh install / after sign-out.
    private(set) var activeNpub: String?

    private(set) var loginError: String?

    /// What kind of identity is currently active.
    enum Mode: String, Sendable, Codable {
        case none
        case localKey
        case remoteSigner
    }
    private(set) var mode: Mode = .none

    /// Weak handle to the Rust kernel (set by `attachKernel`). All key
    /// generation, import, and signing is dispatched here — the kernel is the
    /// single owner of the user's secret.
    @ObservationIgnored weak var kernel: KernelModel?

    /// Test-only: records kernel dispatches instead of reaching the kernel.
    @ObservationIgnored var _kernelDispatchRecorder: (@MainActor (String, [String: Any]) -> Void)?

    /// Test-only: fires each time `dispatchKernelKeygen()` is called, so tests
    /// can verify auto-generation and manual keygen without a live kernel.
    @ObservationIgnored var _keygenCallRecorder: (() -> Void)?

    /// Test-only: fires each time a snapshot pull is requested (from
    /// `dispatchKernelKeygen` or `importNsec`) so tests can verify the pull
    /// path without a live kernel (where `kernel?.requestSnapshotPull()` is nil).
    @ObservationIgnored var _pullCallRecorder: (() -> Void)?

    /// Guards auto-keygen: set on the first `applyKernelIdentity` tick that
    /// finds no active account so subsequent nil ticks don't re-dispatch.
    /// Reset by `clearIdentity()` so sign-out + fresh-install re-enables it.
    @ObservationIgnored private var _autoKeygenDispatched = false

    /// Live state of the NIP-46 connection (UI surfaces this).
    private(set) var remoteSignerState: RemoteSignerState = .idle

    /// Timeout for remote-signer pairing attempts. The kernel may be waiting
    /// on an external app/relay, so the UI must stay pending after dispatch
    /// but still eventually recover if no terminal snapshot arrives.
    @ObservationIgnored var _remoteSignerConnectTimeoutNanoseconds: UInt64 = 60_000_000_000

    /// Client-initiated QR pairing advertises a five-minute expiry in the UI.
    @ObservationIgnored var _nostrConnectTimeoutNanoseconds: UInt64 = 300_000_000_000

    @ObservationIgnored private var _remoteSignerTimeoutTask: Task<Void, Never>?

    /// Cached kind:0 profile fields fetched from relays. `nil` until the
    /// first fetch completes; populated immediately on launch from the
    /// UserDefaults cache so the UI never flashes generated → real.
    var profileDisplayName: String?
    var profileName: String?
    var profileAbout: String?
    var profilePicture: String?

    var hasIdentity: Bool { publicKeyHex != nil }
    var isRemoteSigner: Bool { mode == .remoteSigner }

    // MARK: - Keychain slots (legacy cleanup only)
    //
    // The kernel now owns all key material. These slot identifiers are kept
    // ONLY so `start()` / `clearIdentity()` can delete stale entries written
    // by older installs that stored the user's private key in the Swift
    // Keychain. Nothing in this store ever WRITES a private key to them.

    private static let userKeyService = "\(Bundle.main.bundleIdentifier ?? "Podcastr").user-identity"
    private static let userKeyAccount = "user-private-key-hex"
    private static let userKeyOriginAccount = "user-private-key-origin"
    private static let generatedProfileAccount = "generated-profile-published-pubkey"
    private static let nip46SessionService = "\(Bundle.main.bundleIdentifier ?? "Podcastr").nip46-session"
    private static let nip46SessionAccount = "session-private-key-hex"
    private static let nip46MetaService = "\(Bundle.main.bundleIdentifier ?? "Podcastr").nip46-meta"
    private static let nip46MetaAccount = "connection"

    // MARK: - Lifecycle

    /// Launch hook. The kernel restores its own identity (`identity.json`)
    /// asynchronously and the result arrives via `applyKernelIdentity` on the
    /// first snapshot tick — so this method does NOT eagerly generate a key
    /// (doing so would mint a duplicate account every cold start, racing the
    /// kernel's restore). It only cleans up legacy Swift-Keychain private-key
    /// slots from pre-kernel installs.
    func start() {
        purgeLegacyKeychainKeys()
    }

    /// Delete any private-key material left in the Swift Keychain by older
    /// app versions (the kernel owns keys now). Best-effort; never throws.
    private func purgeLegacyKeychainKeys() {
        try? KeychainStore.deleteString(service: Self.userKeyService, account: Self.userKeyAccount)
        try? KeychainStore.deleteString(service: Self.userKeyService, account: Self.userKeyOriginAccount)
        try? KeychainStore.deleteString(service: Self.nip46SessionService, account: Self.nip46SessionAccount)
        try? KeychainStore.deleteString(service: Self.nip46MetaService, account: Self.nip46MetaAccount)
    }

    // MARK: - nsec import

    /// Import a private key. Dispatches `podcast.identity ImportNsec` to the
    /// app-local identity store (which validates, persists `identity.json`, and
    /// bumps rev so the next push frame carries `active_account`).
    func importNsec(_ nsec: String) throws {
        loginError = nil
        cancelRemoteSignerTimeout()
        let trimmed = nsec.trimmed
        guard !trimmed.isEmpty else {
            loginError = "Invalid nsec — check the key and try again."
            throw UserIdentityError.invalidKey
        }
        remoteSignerState = .idle
        dispatchToKernel(namespace: "podcast.identity", body: ["type": "ImportNsec", "nsec": trimmed])
        _pullCallRecorder?()
        kernel?.requestSnapshotPull()
    }

    // MARK: - Generate account

    /// Generate a brand-new account in the kernel (keypair + kind:0 publish),
    /// activating it as the user's identity. The kernel owns the secret; the
    /// new pubkey arrives via `applyKernelIdentity`.
    func generateKey() throws {
        loginError = nil
        cancelRemoteSignerTimeout()
        remoteSignerState = .idle
        dispatchKernelKeygen()
    }

    /// Dispatch a make-active account-generate to the app-local identity store
    /// via the `podcast.identity Generate` action. The store generates a fresh
    /// keypair, persists it to `PodcastLibrary/identity.json`, registers the
    /// same key as NMP's active signer, and bumps the snapshot rev — so the next
    /// push frame carries `active_account` and Rust-owned publish paths can sign.
    ///
    /// Note: `PodcastApp.createNewAccount` is the NMP multi-account path for
    /// Nostr relay publishing. It does NOT update the app-local `IdentityStore`
    /// that `PodcastUpdate.active_account` reads from and therefore would never
    /// surface the identity in the UI.
    private func dispatchKernelKeygen() {
        _keygenCallRecorder?()
        dispatchToKernel(namespace: "podcast.identity", body: ["type": "Generate"])
        _pullCallRecorder?()
        kernel?.requestSnapshotPull()
    }

    // MARK: - Sign out

    /// Sign out: wipe the active identity from the kernel (and its persisted
    /// `identity.json`) so the key cannot outlive sign-out, and clear any
    /// legacy Swift-Keychain slots. Local published state resets immediately;
    /// `applyKernelIdentity` confirms `activeAccount == nil` on the next tick.
    func clearIdentity() {
        cancelRemoteSignerTimeout()
        try? KeychainStore.deleteString(service: Self.userKeyService, account: Self.generatedProfileAccount)
        purgeLegacyKeychainKeys()
        // Wipe the key from the kernel (else it outlives sign-out in the
        // kernel identity store + identity.json and can still sign).
        clearIdentityInKernel()
        publicKeyHex = nil
        activeNpub = nil
        mode = .none
        remoteSignerState = .idle
        profileDisplayName = nil
        profileName = nil
        profileAbout = nil
        profilePicture = nil
        // Allow the next nil-identity tick to auto-generate a fresh key.
        _autoKeygenDispatched = false
    }

    // MARK: - NIP-46 connect / disconnect

    /// Parse a `bunker://…` URI and begin the NIP-46 handshake via the kernel's
    /// signer broker — no Swift WebSocket. State changes arrive reactively via
    /// `applyKernelIdentity` on kernel snapshot ticks.
    func connectRemoteSigner(uri: String) async {
        loginError = nil
        cancelRemoteSignerTimeout()
        let trimmed = uri.trimmed
        guard trimmed.hasPrefix("bunker://") else {
            loginError = "Invalid bunker URI."
            remoteSignerState = .failed("Invalid bunker URI.")
            return
        }
        remoteSignerState = .connecting
        scheduleRemoteSignerTimeout()
        // Hand off to the kernel — NMP parses, validates, and handles NIP-44,
        // NIP-46, and relay routing. No URI parsing or crypto in Swift.
        syncBunkerToKernel(uri: trimmed)
    }

    /// Disconnect the active remote signer and wipe its kernel identity.
    func disconnectRemoteSigner() async {
        cancelRemoteSignerTimeout()
        clearIdentityInKernel()
        purgeLegacyKeychainKeys()
        publicKeyHex = nil
        activeNpub = nil
        mode = .none
        remoteSignerState = .idle
    }

    // MARK: - Display helpers

    var npub: String? { activeNpub }

    var npubShort: String? {
        guard let full = activeNpub, full.count > 16 else { return activeNpub }
        return "\(full.prefix(10))…\(full.suffix(6))"
    }

    // MARK: - Reactive kernel-identity reconcile

    /// Reconcile published identity state from a kernel snapshot tick. Called
    /// on EVERY tick from `AppStateStore` projection, so it must be cheap and
    /// idempotent: only mutate observable state when a value actually changes,
    /// and only run profile side-effects when the active pubkey changes.
    ///
    /// Ordering matters: handshake (in-flight / terminal) logic runs first so a
    /// tick arriving mid-pairing with `activeAccount == nil` does not reset the
    /// `.connecting` state that `_beginNostrConnect()` set. Steady-state pubkey
    /// reconcile runs second.
    @MainActor
    func applyKernelIdentity(
        handshake: KernelBunkerHandshake?,
        activeNpub: String?,
        pubkeyHex: String?,
        isRemoteSigner: Bool,
        displayName: String? = nil,
        name: String? = nil,
        about: String? = nil,
        pictureUrl: String? = nil
    ) {
        // 1. Handshake progression (failure surfaces an error; success is
        //    folded into the steady-state reconcile below via activeAccount).
        if let handshake, handshake.isFailed {
            _failNostrConnect(handshake.message ?? "NIP-46 pairing failed.")
        }

        // 2. Steady-state: mirror the active identity from the kernel.
        let oldPubkeyHex = publicKeyHex
        if activeNpub != self.activeNpub {
            self.activeNpub = activeNpub
        }
        if pubkeyHex != publicKeyHex {
            publicKeyHex = pubkeyHex
        }

        let newMode: Mode
        if pubkeyHex == nil {
            newMode = .none
        } else {
            newMode = isRemoteSigner ? .remoteSigner : .localKey
        }
        if newMode != mode { mode = newMode }

        applyKernelProfile(
            displayName: displayName,
            name: name,
            about: about,
            pictureUrl: pictureUrl)

        // Connection state: a live account means connected; reflect it without
        // churning if already in the right terminal state.
        if let pubkey = pubkeyHex {
            if isRemoteSigner {
                cancelRemoteSignerTimeout()
                let target: RemoteSignerState = .connected(pubkey)
                if remoteSignerState != target {
                    remoteSignerState = target
                }
            } else if !remoteSignerState.isInFlight {
                let target: RemoteSignerState = .idle
                if remoteSignerState != target {
                    remoteSignerState = target
                }
            }
        }

        // 3. Auto-generate on the first nil-pubkey tick (fresh install / data
        //    reset). One-shot: `_autoKeygenDispatched` prevents re-dispatch on
        //    subsequent nil ticks while the kernel round-trip is in flight.
        if pubkeyHex == nil, !_autoKeygenDispatched, !remoteSignerState.isInFlight {
            _autoKeygenDispatched = true
            dispatchKernelKeygen()
        }

        // 4. Profile side-effects only when the active pubkey actually changes.
        guard oldPubkeyHex != pubkeyHex, let pubkey = pubkeyHex else { return }
        Task { await self.claimProfile(pubkeyHex: pubkey) }
    }

    private func applyKernelProfile(
        displayName: String?,
        name: String?,
        about: String?,
        pictureUrl: String?
    ) {
        let nextDisplayName = nonBlank(displayName)
        let nextName = nonBlank(name)
        let nextAbout = nonBlank(about)
        let nextPicture = nonBlank(pictureUrl)
        if profileDisplayName != nextDisplayName { profileDisplayName = nextDisplayName }
        if profileName != nextName { profileName = nextName }
        if profileAbout != nextAbout { profileAbout = nextAbout }
        if profilePicture != nextPicture { profilePicture = nextPicture }
    }

    private func nonBlank(_ value: String?) -> String? {
        value.flatMap {
            let trimmed = $0.trimmingCharacters(in: .whitespacesAndNewlines)
            return trimmed.isEmpty ? nil : trimmed
        }
    }

    // MARK: - Internal helpers

    /// Internal alias used by `+Publishing.swift` and `FeedbackStore` to
    /// self-heal a missing identity: dispatch kernel keygen so a fresh user
    /// gets an account. The pubkey lands on the next snapshot tick.
    func _ensureGeneratedKey() throws {
        guard publicKeyHex == nil else { return }
        dispatchKernelKeygen()
    }

    func _beginNostrConnect() {
        loginError = nil
        cancelRemoteSignerTimeout()
        remoteSignerState = .connecting
        scheduleRemoteSignerTimeout(nanoseconds: _nostrConnectTimeoutNanoseconds)
    }

    func _failNostrConnect(_ message: String) {
        cancelRemoteSignerTimeout()
        loginError = message
        remoteSignerState = .failed(message)
    }

    private func scheduleRemoteSignerTimeout(
        nanoseconds timeout: UInt64? = nil
    ) {
        let timeout = timeout ?? _remoteSignerConnectTimeoutNanoseconds
        guard timeout > 0 else { return }
        _remoteSignerTimeoutTask?.cancel()
        _remoteSignerTimeoutTask = Task { @MainActor [weak self] in
            do {
                try await Task.sleep(nanoseconds: timeout)
            } catch {
                return
            }
            guard let self, self.remoteSignerState.isInFlight else { return }
            self._failNostrConnect("Remote signer connection timed out.")
        }
    }

    private func cancelRemoteSignerTimeout() {
        _remoteSignerTimeoutTask?.cancel()
        _remoteSignerTimeoutTask = nil
    }

    private static func placeholderGeneratedProfile() -> [String: String] {
        // A neutral display profile for an auto-generated identity. The kernel
        // publishes this kind:0 on account creation; the user can edit it later
        // via the EditProfile flow (`publishProfile`).
        [
            "name": "pod0-user",
            "display_name": "Pod0 User",
            "about": "Feedback identity generated by Pod0.",
        ]
    }

    // MARK: - Test seam

    /// Test-only: directly set the published identity state (no kernel, no
    /// keys), so wiring tests can exercise the `.localKey` readiness path that
    /// gates the kernel publish dispatches.
    func _setActiveAccountForTesting(_ pubkeyHex: String, mode: Mode = .localKey) {
        self.publicKeyHex = pubkeyHex
        let npub = NostrNpub.encode(fromHex: pubkeyHex)
        self.activeNpub = npub == pubkeyHex ? nil : npub
        self.mode = mode
    }

    /// Test-only: drop the active identity (verifies the self-heal path).
    func _clearActiveAccountForTesting() {
        self.publicKeyHex = nil
        self.activeNpub = nil
        self.mode = .none
    }
}

// MARK: - Supporting types

/// Connection state surfaced to the UI for the NIP-46 flow.
enum RemoteSignerState: Sendable, Equatable {
    case idle
    case connecting
    case reconnecting
    /// Bunker replied with an `auth_url` challenge — user must approve in a
    /// browser; `connected(...)` follows once the bunker delivers the `ack`.
    case awaitingAuthorization(URL)
    case connected(String)            // associated value: user pubkey hex
    case failed(String)               // error message

    var isInFlight: Bool {
        switch self {
        case .connecting, .reconnecting, .awaitingAuthorization:
            true
        case .idle, .connected, .failed:
            false
        }
    }
}

enum UserIdentityError: LocalizedError {
    case noIdentity
    case invalidKey
    /// The Rust kernel synchronously rejected a `podcast.social` dispatch
    /// (e.g. no active account, kernel not attached). Carries the rejection
    /// message from `DispatchResult.failure`.
    case dispatchRejected(_ message: String)

    var errorDescription: String? {
        switch self {
        case .noIdentity:
            "No feedback identity is available."
        case .invalidKey:
            "Invalid key — check the value and try again."
        case let .dispatchRejected(message):
            "Publish rejected: \(message)"
        }
    }
}
