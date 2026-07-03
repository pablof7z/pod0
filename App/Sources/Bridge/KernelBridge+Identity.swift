import Foundation

// MARK: - PodcastHandle identity / NIP-46 wrappers
//
// Every wrapper is fire-and-forget (D6) — synchronous return value is
// either void or, in the nostrconnect URI case, an optional string. The success
// / failure outcome of the underlying actor command is observable through
// `KernelIdentityProjection`, NOT through these calls.
//
// SECURITY: `nsec` is the user's bech32 private key. NEVER log it. NEVER
// store it outside the Rust-side `Zeroizing<String>` the actor wraps it in
// inside `PodcastApp.signinNsec`. Persistence is owned by the kernel's
// `nmp.identity.local_nsec.*` keyring entries — `PcstIdentityCapability` MUST
// NOT also retain a copy.

extension PodcastHandle {
    /// Enqueue `ActorCommand::SignInBunker` with the supplied `bunker://`
    /// URI. Silent no-op (D6) if the bunker hook has not been installed,
    /// which is why `PodcastHandle.init` calls `PodcastApp.signerBrokerInit`.
    func signInBunker(uri: String) {
        podcastApp.signinBunker(uri: uri, makeActive: true)
    }

    /// Enqueue `ActorCommand::SignInNsec` with the supplied bech32 / hex
    /// secret. The Rust side wraps it in `Zeroizing<String>` immediately
    /// upon copy-in.
    func signInNsec(_ nsec: String) {
        podcastApp.signinNsec(secret: nsec, makeActive: true)
    }

    /// Cancel the in-flight NIP-46 handshake. Idempotent / safe when nothing
    /// is in flight.
    func cancelBunkerHandshake() {
        podcastApp.cancelBunkerHandshake()
    }

    /// Generate a brand-new account (keypair) inside the kernel, publish its
    /// kind:0 profile + relay list, and — when `makeActive` is true — switch
    /// the active session to it. The kernel owns the generated secret; Swift
    /// never sees private bytes. The resulting pubkey surfaces on the next
    /// snapshot tick via `KernelIdentityProjection.activeAccount` (when
    /// `makeActive`). Fire-and-forget (D6).
    ///
    /// `profileJSON` is a flat string-map (`{"name":"…","display_name":"…"}`);
    /// `relaysJSON` is `[[url, role], …]`. Pass `"{}"` / `"[]"` for kernel
    /// defaults.
    func createNewAccount(
        profileJSON: String,
        relaysJSON: String,
        mls: Bool,
        makeActive: Bool
    ) {
        podcastApp.createNewAccount(
            profileJson: profileJSON,
            relaysJson: relaysJSON,
            mls: mls,
            makeActive: makeActive
        )
    }

    /// Allocate a freshly generated `nostrconnect://` URI from the broker,
    /// copy it to a Swift `String`, and free the C buffer. Returns `nil`
    /// when the broker is not initialised or Rust returned a null pointer.
    /// The relay is kernel-selected from the configured write relays. The
    /// optional callback scheme is passed through so signer apps can deep-link
    /// back after approval.
    func nostrconnectURI(relayURL: String?, callbackScheme: String?) -> String? {
        podcastApp.nostrconnectUri(callbackScheme: callbackScheme)
    }

    /// Enqueue `ActorCommand::RemoveAccount` for `identityId`. The actor
    /// rebuilds the kernel snapshot with the row dropped on the next tick.
    func removeAccount(identityId: String) {
        podcastApp.removeAccount(identityId: identityId)
    }

    /// Register a refcounted interest in `pubkeyHex`'s kind:0 profile under
    /// `consumerID`. The kernel fetches the profile over its own relay pool
    /// (cold claim) and surfaces it in `projections.resolved_profiles` on the
    /// next snapshot tick — the same push `mergeResolvedProfiles` folds into
    /// `nostrProfileCache`. Fire-and-forget (D6): an invalid pubkey is a no-op.
    ///
    /// Uses ADR-0063 Lane D ref resolution with namespace=profile,
    /// shape=profile.card, liveness=Live for open-screen claims.
    func claimProfile(pubkeyHex: String, consumerID: String) {
        podcastApp.resolveRef(
            namespace: .profile,
            key: pubkeyHex,
            consumerId: consumerID,
            shape: .profile(shape: .card),
            liveness: .live
        )
    }

    /// Release a previously-claimed profile interest. The kernel drops the
    /// pending request when the last consumer releases. Mirrors `claimProfile`.
    ///
    /// Uses ADR-0063 Lane D ref release with namespace=profile.
    func releaseProfile(pubkeyHex: String, consumerID: String) {
        podcastApp.releaseRef(namespace: .profile, key: pubkeyHex, consumerId: consumerID)
    }
}
