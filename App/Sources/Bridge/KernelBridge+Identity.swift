import Foundation

// MARK: - PodcastHandle identity / NIP-46 wrappers
//
// Typed Swift wrappers around the `nmp_app_signin_*` and
// `nmp_app_*_bunker*` / `nmp_app_nostrconnect_uri` C symbols declared in
// `NmpCore.h`. `KernelModel` calls these so it does not need to reach into
// the raw `PodcastHandle.raw` pointer at every identity-related touchpoint.
//
// Every wrapper is fire-and-forget (D6) — synchronous return value is
// either void or, in the nostrconnect URI case, a heap C string the wrapper
// owns and frees before returning. The success / failure outcome of the
// underlying actor command is observable through
// `KernelIdentityProjection`, NOT through these calls.
//
// SECURITY: `nsec` is the user's bech32 private key. NEVER log it. NEVER
// store it outside the Rust-side `Zeroizing<String>` the actor wraps it in
// (`crates/nmp-ffi/src/identity.rs::nmp_app_signin_nsec`). Persistence is
// owned by the kernel's `nmp.identity.local_nsec.*` keyring entries —
// `PcstIdentityCapability` MUST NOT also retain a copy.

extension PodcastHandle {
    /// Enqueue `ActorCommand::SignInBunker` with the supplied `bunker://`
    /// URI. Silent no-op (D6) if the bunker hook has not been installed
    /// (i.e. `nmp_signer_broker_init` was never called) — which is why we
    /// call the init from `PodcastHandle.init` itself.
    func signInBunker(uri: String) {
        uri.withCString { nmp_app_signin_bunker(raw, $0) }
    }

    /// Enqueue `ActorCommand::SignInNsec` with the supplied bech32 / hex
    /// secret. The Rust side wraps it in `Zeroizing<String>` immediately
    /// upon copy-in.
    func signInNsec(_ nsec: String) {
        nsec.withCString { nmp_app_signin_nsec(raw, $0) }
    }

    /// Cancel the in-flight NIP-46 handshake. Idempotent / safe when nothing
    /// is in flight.
    func cancelBunkerHandshake() {
        nmp_app_cancel_bunker_handshake(raw)
    }

    /// Allocate a freshly generated `nostrconnect://` URI from the broker,
    /// copy it to a Swift `String`, and free the C buffer. Returns `nil`
    /// when the broker is not initialised or Rust returned a null pointer.
    /// `relayURL` / `callbackScheme` are passed through verbatim — `nil`
    /// for either means "use the Rust-side default" (kernel-selected relay
    /// or no callback respectively).
    func nostrconnectURI(relayURL: String?, callbackScheme: String?) -> String? {
        let relayCStr = relayURL?.withCString(strdup)
        let callbackCStr = callbackScheme?.withCString(strdup)
        defer {
            if let relayCStr { free(relayCStr) }
            if let callbackCStr { free(callbackCStr) }
        }
        guard let ptr = nmp_app_nostrconnect_uri(
            raw,
            relayCStr.map { UnsafePointer($0) },
            callbackCStr.map { UnsafePointer($0) })
        else { return nil }
        defer { nmp_broker_free_string(ptr) }
        return String(cString: ptr)
    }

    /// Enqueue `ActorCommand::RemoveAccount` for `identityId`. The actor
    /// rebuilds the kernel snapshot with the row dropped on the next tick.
    func removeAccount(identityId: String) {
        identityId.withCString { nmp_app_remove_account(raw, $0) }
    }

    /// Register a refcounted interest in `pubkeyHex`'s kind:0 profile under
    /// `consumerID`. The kernel fetches the profile over its own relay pool
    /// (cold claim) and surfaces it in `projections.resolved_profiles` on the
    /// next snapshot tick — the same push `mergeResolvedProfiles` folds into
    /// `nostrProfileCache`. Fire-and-forget (D6): an invalid pubkey is a no-op.
    func claimProfile(pubkeyHex: String, consumerID: String) {
        pubkeyHex.withCString { pk in
            consumerID.withCString { cid in
                nmp_app_claim_profile(raw, pk, cid)
            }
        }
    }

    /// Release a previously-claimed profile interest. The kernel drops the
    /// pending request when the last consumer releases. Mirrors `claimProfile`.
    func releaseProfile(pubkeyHex: String, consumerID: String) {
        pubkeyHex.withCString { pk in
            consumerID.withCString { cid in
                nmp_app_release_profile(raw, pk, cid)
            }
        }
    }
}
