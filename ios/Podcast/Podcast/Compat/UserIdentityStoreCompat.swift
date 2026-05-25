// `UserIdentityStore` — kernel-snapshot-backed identity surface for the
// migrated SwiftUI views (Identity / Onboarding / Settings / Advanced).
//
// The store is a thin reactive façade over `KernelModel.identity` (the NMP
// kernel `projections.active_account` / `projections.accounts` /
// `projections.bunker_handshake` slice). Views still bind to the legacy
// `@Observable` surface (`publicKeyHex`, `mode`, `remoteSignerState`,
// `isRemoteSigner`, …); those are computed properties that read through to
// the bound `KernelModel`, so Observation tracking re-renders the view
// whenever the underlying kernel snapshot changes — no separate observer
// task is required.
//
// Sign-in / sign-out methods (`importNsec`, `connectRemoteSigner`,
// `connectViaNostrConnect`, `disconnectRemoteSigner`, `clearIdentity`)
// dispatch through `KernelModel`'s typed identity wrappers, which in turn
// hit the NMP-core actor over the C FFI. Persistence (nsec storage, remote-
// signer session payloads) is owned by the kernel keyring path — the iOS
// shell MUST NOT also retain the secret.

import Foundation
import Observation
import SwiftUI

// MARK: - Signer marker protocol

/// Marker protocol — `NostrSigner` is referenced as a type by
/// `BlossomUploading.upload(data:contentType:signer:)` and as the type of
/// `UserIdentityStore.signer`. No methods are called on it in the migrated
/// views, so an empty marker protocol is sufficient.
///
/// Declared `Sendable` so `ChangePhotoSheet.handlePicked` can pass it across
/// a `Task` boundary into `BlossomUploading.upload`.
protocol NostrSigner: AnyObject, Sendable {}

// MARK: - Remote signer state

enum RemoteSignerState: Sendable, Equatable {
    case idle
    case connecting
    case reconnecting
    case awaitingAuthorization(URL)
    case connected(String)
    case failed(String)
}

// MARK: - User identity store

/// Reactive identity facade. Read-only fields (`publicKeyHex`, `mode`,
/// `remoteSignerState`, …) are computed properties that read through the
/// bound `KernelModel.identity`; Observation tracking automatically picks
/// up the dependency. Mutating methods dispatch through the kernel FFI and
/// rely on the next snapshot tick to confirm the new state.
@MainActor
@Observable
final class UserIdentityStore {

    enum Mode: String, Sendable, Codable {
        case none
        case localKey
        case remoteSigner
    }

    // MARK: - Profile metadata
    //
    // `profileDisplayName` / `profilePicture` are computed: the kernel
    // kind:0 value (from `projections.accounts[active].display_name` /
    // `.picture_url`) takes precedence; a local stored override receives
    // writes from `EditProfileView` so the form re-hydrates correctly.
    //
    // `profileName` / `profileAbout` are kind:0 fields the kernel doesn't
    // yet expose through the accounts projection — stored as plain vars
    // for the EditProfile form's write-through.

    var profileName: String?
    var profileAbout: String?
    var loginError: String?

    /// Backing stored override for `profileDisplayName`. The computed
    /// property below prefers the kernel kind:0 value when present.
    @ObservationIgnored private var storedProfileDisplayName: String?
    /// Backing stored override for `profilePicture`. Same precedence rule
    /// as `profileDisplayName`.
    @ObservationIgnored private var storedProfilePicture: String?

    var profileDisplayName: String? {
        get {
            // Prefer the kernel-sourced value so a NIP-46 sign-in that
            // synced the user's kind:0 immediately shows their name; fall
            // back to the EditProfile-set override (and finally `nil`).
            kernel?.identity.activeAccountRow?.displayName ?? storedProfileDisplayName
        }
        set { storedProfileDisplayName = newValue }
    }

    var profilePicture: String? {
        get {
            kernel?.identity.activeAccountRow?.pictureUrl ?? storedProfilePicture
        }
        set { storedProfilePicture = newValue }
    }

    // MARK: - Kernel binding

    /// Weak handle to the `KernelModel` whose snapshot drives this store.
    /// Stored via `@ObservationIgnored` so Observation doesn't track the
    /// back-reference itself — only the reads of `kernel?.identity.*` get
    /// recorded as dependencies, which is exactly the dependency edge SwiftUI
    /// needs.
    @ObservationIgnored private weak var kernel: KernelModel?

    /// `true` once the user has explicitly initiated an outbound NIP-46
    /// handshake during this app session, before the broker has emitted its
    /// first `BunkerHandshakeProgress` frame. Disambiguates the "no active
    /// account + idle handshake" steady state (render as `.idle`) from
    /// "user just tapped Connect" (render as `.connecting`). Cleared as
    /// soon as the kernel emits a handshake stage or the connect resolves.
    var localHandshakeInFlight: Bool = false

    /// Bind to `kernel`. Idempotent — a second call replaces the prior
    /// binding. No background work is started; the field reads above
    /// register their own Observation dependencies through `kernel.identity`
    /// each render.
    func bind(kernel: KernelModel) {
        self.kernel = kernel
    }

    /// Drop the kernel binding. Safe to call when never bound.
    func unbind() {
        self.kernel = nil
    }

    // MARK: - Computed snapshot mirrors

    /// Hex pubkey of the active identity, or `nil` when none is loaded.
    var publicKeyHex: String? {
        kernel?.identity.activeAccount
    }

    /// `local` / `remoteSigner` derived from `signer_is_remote` on the
    /// active account row (per NMP doctrine — never string-match
    /// `signer_kind`). `.none` when no account is active.
    var mode: Mode {
        guard let row = kernel?.identity.activeAccountRow else { return .none }
        return row.signerIsRemote ? .remoteSigner : .localKey
    }

    var isRemoteSigner: Bool { mode == .remoteSigner }
    var hasIdentity: Bool { publicKeyHex != nil }

    /// Bech32 `npub1…` if the active account is loaded, otherwise the raw
    /// hex (for crash-safety on pre-snapshot renders).
    var npub: String? {
        kernel?.identity.activeAccountRow?.npub ?? publicKeyHex
    }

    var npubShort: String? {
        guard let full = npub, full.count > 16 else { return npub }
        return "\(full.prefix(10))…\(full.suffix(6))"
    }

    /// Sentinel signer token. The Rust side owns the actual signer; this
    /// is only used by the upload path to decide whether a signer is
    /// present at all. Returns the shared `RemoteSignerToken` instance
    /// when a NIP-46 account is active, `nil` otherwise.
    var signer: (any NostrSigner)? {
        isRemoteSigner ? RemoteSignerToken.shared : nil
    }

    /// Pre-derived state machine for the NIP-46 remote-signer surface. Pure
    /// function of the bound kernel's `bunkerHandshake` slot + active-account
    /// flag + the locally-tracked optimistic in-flight bit.
    var remoteSignerState: RemoteSignerState {
        Self.derive(
            handshake: kernel?.identity.bunkerHandshake,
            isRemoteSigner: kernel?.identity.isRemoteSigner ?? false,
            localInFlight: localHandshakeInFlight)
    }

    private static func derive(
        handshake: KernelBunkerHandshake?,
        isRemoteSigner: Bool,
        localInFlight: Bool
    ) -> RemoteSignerState {
        if let dto = handshake {
            if dto.isFailed {
                return .failed(dto.message ?? dto.stageLabel)
            }
            if dto.isInFlight {
                return .connecting
            }
            if dto.isTerminalSuccess {
                let label = (dto.message?.isEmpty == false) ? dto.message! : dto.stageLabel
                return .connected(label)
            }
        }
        if isRemoteSigner {
            return .connected("Connected")
        }
        if localInFlight {
            return .connecting
        }
        return .idle
    }

    // MARK: - Lifecycle

    /// Idempotent. `bind(kernel:)` does the real wiring; this exists for
    /// API symmetry with the prior compat shim.
    func start() {}

    /// Remove the active account from the kernel and clear local profile
    /// caches. The actual snapshot mutation lands on the next tick.
    func clearIdentity() {
        kernel?.removeActiveAccount()
        storedProfileDisplayName = nil
        profileName = nil
        profileAbout = nil
        storedProfilePicture = nil
        loginError = nil
        localHandshakeInFlight = false
    }

    // MARK: - Sign-in

    /// Validate-then-dispatch an nsec import. The pre-flight is intentionally
    /// permissive — the kernel does the authoritative bech32-checksum check.
    /// A malformed key that passes pre-flight is dropped by the actor with
    /// a `last_error_toast`; surfacing that toast through the iOS UI is a
    /// follow-up (today the user sees a silent no-op for invalid keys that
    /// match the `nsec1…` prefix shape).
    ///
    /// SECURITY: the nsec MUST NOT be logged. It is forwarded straight to
    /// the kernel FFI which copies it into `Zeroizing<String>` on the
    /// other side.
    func importNsec(_ nsec: String) throws {
        let trimmed = nsec.trimmingCharacters(in: .whitespacesAndNewlines)
        guard isPlausibleNsec(trimmed) else {
            loginError = "That key doesn't look right."
            throw IdentityError.invalidKey
        }
        loginError = nil
        kernel?.signInNsec(trimmed)
        // Local-key sign-in is synchronous on the actor side; the next
        // snapshot tick will populate `publicKeyHex` + `mode = .localKey`.
    }

    /// Generate a fresh local key. Not implemented in this PR — surfaced as
    /// an unsupported-action error so callers render an inline message
    /// rather than throwing.
    func generateKey() throws {
        throw IdentityError.unsupported("Generating a new key is not available yet.")
    }

    /// Dispatch a `bunker://` connection. The broker's progress events flow
    /// back through `projections.bunker_handshake` on subsequent ticks; the
    /// computed `remoteSignerState` re-derives from there.
    func connectRemoteSigner(uri: String) async {
        let trimmed = uri.trimmingCharacters(in: .whitespacesAndNewlines)
        guard trimmed.hasPrefix("bunker://") else {
            loginError = "That doesn't look like a bunker:// link. Check the link and try again."
            return
        }
        localHandshakeInFlight = true
        loginError = nil
        kernel?.signInBunker(uri: trimmed)
        // Yield so the caller's spinner sees the `.connecting` transition
        // before the await returns. The view also observes the state
        // directly, so this is purely a UX nicety.
        await Task.yield()
    }

    /// Cancel the in-flight handshake. Does NOT remove an already-active
    /// remote-signer account — for that, the user goes through
    /// `clearIdentity` (the Identity > Sign Out button). Disconnect mid-
    /// pairing is the only behavior here so an accidental cancel during
    /// re-pairing doesn't nuke the existing session.
    func disconnectRemoteSigner() async {
        kernel?.cancelBunkerHandshake()
        localHandshakeInFlight = false
    }

    /// NIP-46 `nostrconnect://` pairing initiator. The broker is already
    /// listening for the signer-app response on the embedded relay; the
    /// host's job is to hand the user the URI. The callback fires once
    /// (or never, if the broker is uninitialised) with the wire string.
    ///
    /// Callback URL scheme is intentionally `nil` here — the iOS host has
    /// NOT registered `podcastr://` with the OS yet, so a deep-link return
    /// trip would fail silently. Tracked as follow-up; the QR-scan path
    /// still works because the signer app does NOT require a return URL.
    func connectViaNostrConnect(onURI: @escaping (String) async -> Void) async {
        localHandshakeInFlight = true
        loginError = nil
        guard let uri = kernel?.nostrconnectURI(relayURL: nil, callbackScheme: nil) else {
            localHandshakeInFlight = false
            loginError = "Couldn't generate a pairing link. Try again in a moment."
            return
        }
        await onURI(uri)
        // Stage transitions thereafter arrive through
        // `projections.bunker_handshake` on the kernel update channel.
    }

    // MARK: - Profile publish (legacy entry point)

    /// Preserved for any consumer that still holds a reference to
    /// `UserIdentityStore` without a `KernelModel` in scope. The active
    /// publish path now lives in `EditProfileView.save()`, which dispatches
    /// `nmp.publish` `PublishProfile` through the kernel directly and persists
    /// fields to `@AppStorage("agent.profile.*")`. This stub remains so the
    /// compat-deletion follow-up has one less surface to chase.
    // MARK: - Profile publish

    /// Local-only profile cache write. Real kind:0 publish lands in a
    /// separate PR (`pr-profile-editing`); this stub keeps the existing
    /// EditProfile view compiling without throwing.
    @discardableResult
    func publishProfile(
        name: String,
        displayName: String,
        about: String,
        picture: String
    ) async throws -> String {
        profileName = name
        profileDisplayName = displayName
        profileAbout = about
        profilePicture = picture
        return ""
    }

    // MARK: - Helpers

    /// Cheap pre-flight bech32 shape check. The kernel performs the real
    /// checksum validation; this only filters obvious typos so we don't
    /// hand the FFI an empty string.
    private func isPlausibleNsec(_ candidate: String) -> Bool {
        guard candidate.hasPrefix("nsec1") else { return false }
        return candidate.count >= 60 && candidate.count <= 70
    }
}

// MARK: - Sentinel signer

/// Opaque sentinel handed back via `UserIdentityStore.signer` when a remote
/// signer is active. Carries no data — the actual signing happens on the
/// Rust actor side, and the upload path only needs a non-nil token to
/// branch on "do we have a signer at all?".
private final class RemoteSignerToken: NostrSigner {
    static let shared = RemoteSignerToken()
    private init() {}
}

// MARK: - Identity error

enum IdentityError: LocalizedError {
    case invalidKey
    case unsupported(String)

    var errorDescription: String? {
        switch self {
        case .invalidKey:
            return "That key doesn't look right. Check the start (it should begin with nsec1) and try again."
        case .unsupported(let msg):
            return msg
        }
    }
}

// MARK: - Compat error retention

/// Retained for any callers that still reference the legacy `CompatError`
/// thrown by the M1.E shim. New code should use `IdentityError` instead.
enum CompatError: LocalizedError {
    case notImplemented(String)

    var errorDescription: String? {
        switch self {
        case .notImplemented(let symbol):
            return "\(symbol) is not yet implemented."
        }
    }
}
