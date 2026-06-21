import Foundation

// MARK: - KernelModel identity, account, and profile accessors
//
// Typed wrappers around the NMP-core identity FFI, the opaque podcast-handle
// pointer, the action-results registry, and the reference-first profile
// resolution seam. Extracted from KernelModel.swift to keep that file under
// the AGENTS.md 500-line hard limit.
//
// `UserIdentityStore` calls these instead of touching the raw `PodcastHandle`.
// The actor confirms the resulting state on the next snapshot tick via
// `KernelIdentityProjection` — no synchronous return.

extension KernelModel {

    // ── Provider-blind LLM chat completion ──────────────────────────────────

    /// Resolve the opaque podcast handle pointer for use in a blocking FFI call.
    /// The pointer is stable for the process lifetime once registered (D6).
    /// Returns nil when the kernel is not yet registered.
    var podcastHandlePointer: UnsafeMutableRawPointer? {
        kernel.podcastHandle
    }

    // ── Identity / NIP-46 ────────────────────────────────────────────────────

    /// Begin a `bunker://` sign-in. Fire-and-forget — observe
    /// `identity.bunkerHandshake` / `identity.activeAccount` for outcome.
    /// Silent no-op (D6) if `nmp_signer_broker_init` was never called.
    func signInBunker(uri: String) {
        kernel.signInBunker(uri: uri)
    }

    /// Begin an nsec sign-in. The secret crosses the FFI boundary as raw
    /// bytes (it has to be imported somehow) and is wrapped in `Zeroizing`
    /// the instant the actor receives it (see
    /// `crates/nmp-ffi/src/identity.rs::nmp_app_signin_nsec`). The Rust
    /// `ActorCommand::SignInNsec` handler validates and persists the key
    /// via the kernel keyring path — DO NOT also write to
    /// `PcstIdentityCapability` here. Single source of truth.
    func signInNsec(_ nsec: String) {
        kernel.signInNsec(nsec)
    }

    /// Public seam so callers outside KernelModel can request a snapshot
    /// pull (e.g. after `createNewAccount` or `signInNsec`). The underlying
    /// pull is rev-gated — safe to call redundantly.
    func requestSnapshotPull() {
        pullPodcastSnapshotIfChanged()
    }

    /// Generate a fresh account in the kernel (keypair + kind:0 publish). The
    /// kernel owns the secret; Swift never holds private bytes. When
    /// `makeActive` is true the new account becomes the active session and its
    /// pubkey arrives on the next snapshot tick via
    /// `kernelIdentity.activeAccount`. `profile` is a flat string-map and
    /// `relays` is a list of `[url, role]` pairs; both default to kernel
    /// defaults when omitted.
    func createNewAccount(
        profile: [String: String] = [:],
        relays: [[String]] = [],
        mls: Bool = false,
        makeActive: Bool = true
    ) {
        let profileJSON = (try? JSONSerialization.data(withJSONObject: profile, options: [.sortedKeys]))
            .flatMap { String(data: $0, encoding: .utf8) } ?? "{}"
        let relaysJSON = (try? JSONSerialization.data(withJSONObject: relays, options: []))
            .flatMap { String(data: $0, encoding: .utf8) } ?? "[]"
        kernel.createNewAccount(
            profileJSON: profileJSON,
            relaysJSON: relaysJSON,
            mls: mls,
            makeActive: makeActive
        )
    }

    /// Cancel the in-flight bunker handshake. Safe / idempotent when nothing
    /// is in flight.
    func cancelBunkerHandshake() {
        kernel.cancelBunkerHandshake()
    }

    /// Generate a fresh `nostrconnect://` URI for client-initiated NIP-46
    /// pairing. The broker is already listening for the signer app's
    /// response on the embedded relay — handing the URI to the user (QR or
    /// deep-link) is the only host responsibility. `callbackScheme` should
    /// be `nil` when the host URL scheme is not registered with the OS.
    func nostrconnectURI(relayURL: String? = nil, callbackScheme: String? = nil) -> String? {
        kernel.nostrconnectURI(relayURL: relayURL, callbackScheme: callbackScheme)
    }

    /// Remove the active account from the kernel. Mirrored on the next
    /// snapshot tick via `identity.activeAccount` flipping to `nil`.
    func removeActiveAccount() {
        guard let active = kernelIdentity.activeAccount else { return }
        kernel.removeAccount(identityId: active)
    }

    // ── action_results registry (nmp.blossom.upload + future async actions) ──

    /// Drain-once resolver for async-completing kernel actions. Populated by
    /// the `nmpUpdateCallback` on every push frame that carries
    /// `projections["action_results"]`. `blossomUpload` awaits its
    /// correlation-id here. Exposed `internal` so `KernelModel+BlossomUpload`
    /// can capture it without accessing the private `kernel` property.
    var actionResultsRegistry: ActionResultsRegistry { kernel.actionResultsRegistry }

    // Blossom kernel upload lives in KernelModel+BlossomUpload.swift to keep
    // this file under the AGENTS.md 500-line soft limit.

    // ── Profile resolution (reference-first; rides resolved_profiles) ────────
    //
    // Replaces the host opening its own websocket to fetch kind:0. A view that
    // displays a Nostr profile claims the pubkey on appear and releases on
    // disappear; the kernel fetches kind:0 over its own relay pool and delivers
    // the result via `projections.resolved_profiles`, which
    // `AppStateStore.mergeResolvedProfiles` folds into `nostrProfileCache`. The
    // display then re-renders reactively. `consumerID` is a stable per-view
    // token so the kernel's refcount dedupes and release matches claim.

    /// Claim a refcounted interest in `pubkeyHex`'s kind:0 profile.
    func claimProfile(pubkeyHex: String, consumerID: String) {
        kernel.claimProfile(pubkeyHex: pubkeyHex, consumerID: consumerID)
    }

    /// Release a previously-claimed profile interest.
    func releaseProfile(pubkeyHex: String, consumerID: String) {
        kernel.releaseProfile(pubkeyHex: pubkeyHex, consumerID: consumerID)
    }
}
