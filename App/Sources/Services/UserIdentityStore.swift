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
    /// `PodcastUpdate.active_account.pubkey_hex`. `nil` while no identity is
    /// configured or while the kernel predates the `pubkey_hex` projection (older
    /// builds emit only `npub`; use `activeNpub` as the identity presence check).
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

    /// Live state of the NIP-46 connection (UI surfaces this).
    private(set) var remoteSignerState: RemoteSignerState = .idle

    /// Cached kind:0 profile fields fetched from relays. `nil` until the
    /// first fetch completes; populated immediately on launch from the
    /// UserDefaults cache so the UI never flashes generated → real.
    var profileDisplayName: String?
    var profileName: String?
    var profileAbout: String?
    var profilePicture: String?

    var hasIdentity: Bool { activeNpub != nil }
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

    /// Import a private key. The kernel validates, persists, and adopts it as
    /// the active account; the resulting pubkey arrives via `applyKernelIdentity`.
    func importNsec(_ nsec: String) throws {
        loginError = nil
        let trimmed = nsec.trimmed
        guard !trimmed.isEmpty else {
            loginError = "Invalid nsec — check the key and try again."
            throw UserIdentityError.invalidKey
        }
        remoteSignerState = .idle
        kernel?.signInNsec(trimmed)
    }

    // MARK: - Generate account

    /// Generate a brand-new account in the kernel (keypair + kind:0 publish),
    /// activating it as the user's identity. The kernel owns the secret; the
    /// new pubkey arrives via `applyKernelIdentity`.
    func generateKey() throws {
        loginError = nil
        remoteSignerState = .idle
        dispatchKernelKeygen()
    }

    /// Dispatch a make-active account creation to the kernel, seeding the
    /// auto-generated display profile so the kernel's kind:0 publish carries
    /// it (avoids a second Swift-side profile publish).
    private func dispatchKernelKeygen() {
        kernel?.createNewAccount(
            profile: Self.placeholderGeneratedProfile(),
            relays: [],
            mls: false,
            makeActive: true
        )
    }

    // MARK: - Sign out

    /// Sign out: wipe the active identity from the kernel (and its persisted
    /// `identity.json`) so the key cannot outlive sign-out, and clear any
    /// legacy Swift-Keychain slots. Local published state resets immediately;
    /// `applyKernelIdentity` confirms `activeAccount == nil` on the next tick.
    func clearIdentity() {
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
    }

    // MARK: - NIP-46 connect / disconnect

    /// Parse a `bunker://…` URI and begin the NIP-46 handshake via the kernel's
    /// signer broker — no Swift WebSocket. State changes arrive reactively via
    /// `applyKernelIdentity` on kernel snapshot ticks.
    func connectRemoteSigner(uri: String) async {
        loginError = nil
        let trimmed = uri.trimmed
        guard trimmed.hasPrefix("bunker://") else {
            loginError = "Invalid bunker URI."
            remoteSignerState = .failed("Invalid bunker URI.")
            return
        }
        remoteSignerState = .connecting
        // Hand off to the kernel — NMP parses, validates, and handles NIP-44,
        // NIP-46, and relay routing. No URI parsing or crypto in Swift.
        syncBunkerToKernel(uri: trimmed)
    }

    /// Disconnect the active remote signer and wipe its kernel identity.
    func disconnectRemoteSigner() async {
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
        activeAccount: String?,       // bech32 npub — used for display + presence
        pubkeyHex: String?,            // hex pubkey — used for crypto / relay queries
        isRemoteSigner: Bool
    ) {
        // 1. Handshake progression (failure surfaces an error; success is
        //    folded into the steady-state reconcile below via activeAccount).
        if let handshake, handshake.isFailed {
            _failNostrConnect(handshake.message ?? "NIP-46 pairing failed.")
        }

        // 2. Steady-state: mirror the active identity from the kernel.
        let changed = (activeAccount != activeNpub)
        if changed {
            activeNpub = activeAccount
            publicKeyHex = pubkeyHex
        } else if pubkeyHex != publicKeyHex {
            // Hex may arrive on a tick after npub on kernels that add pubkey_hex
            // mid-session (e.g. after an app update that ships this projection).
            publicKeyHex = pubkeyHex
        }

        let newMode: Mode
        if activeAccount == nil {
            newMode = .none
        } else {
            newMode = isRemoteSigner ? .remoteSigner : .localKey
        }
        if newMode != mode { mode = newMode }

        // Connection state: a live account means connected; reflect it without
        // churning if already in the right terminal state.
        if let npub = activeAccount {
            let target: RemoteSignerState = isRemoteSigner ? .connected(npub) : .idle
            if remoteSignerState != target {
                // Don't downgrade an in-flight bunker handshake to idle on a
                // local-key tick; only set when we have a real account.
                remoteSignerState = target
            }
        }

        // 3. Profile side-effects only when the active pubkey actually changes.
        // Prefer the hex key for profile cache keying; fall back to the npub so
        // older kernel snapshots that omit pubkey_hex still populate the cache.
        guard changed, let cacheKey = pubkeyHex ?? activeAccount else { return }
        loadCachedProfile(for: cacheKey)
        Task { await self.fetchAndCacheProfile(pubkeyHex: cacheKey) }
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
        remoteSignerState = .connecting
    }

    func _failNostrConnect(_ message: String) {
        loginError = message
        remoteSignerState = .failed(message)
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
        self.activeNpub = pubkeyHex  // tests pass hex; presence check still works
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
}

enum UserIdentityError: LocalizedError {
    case noIdentity
    case invalidKey

    var errorDescription: String? {
        switch self {
        case .noIdentity:
            "No feedback identity is available."
        case .invalidKey:
            "Invalid key — check the value and try again."
        }
    }
}
