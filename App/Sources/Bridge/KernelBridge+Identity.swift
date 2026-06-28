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
        uri.withCString { nmp_app_signin_bunker(raw, $0, 1) }
    }

    /// Enqueue `ActorCommand::SignInNsec` with the supplied bech32 / hex
    /// secret. The Rust side wraps it in `Zeroizing<String>` immediately
    /// upon copy-in.
    func signInNsec(_ nsec: String) {
        nsec.withCString { nmp_app_signin_nsec(raw, $0, 1) }
    }

    /// Cancel the in-flight NIP-46 handshake. Idempotent / safe when nothing
    /// is in flight.
    func cancelBunkerHandshake() {
        nmp_app_cancel_bunker_handshake(raw)
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
        profileJSON.withCString { profile in
            relaysJSON.withCString { relays in
                nmp_app_create_new_account(
                    raw,
                    profile,
                    relays,
                    mls,
                    makeActive ? 1 : 0
                )
            }
        }
    }

    /// Allocate a freshly generated `nostrconnect://` URI from the broker,
    /// copy it to a Swift `String`, and free the C buffer. Returns `nil`
    /// when the broker is not initialised or Rust returned a null pointer.
    /// The relay is kernel-selected from the configured write relays. The
    /// optional callback scheme is passed through so signer apps can deep-link
    /// back after approval.
    func nostrconnectURI(relayURL: String?, callbackScheme: String?) -> String? {
        let callbackCStr = callbackScheme?.withCString(strdup)
        defer {
            if let callbackCStr { free(callbackCStr) }
        }
        guard let ptr = nmp_app_nostrconnect_uri(
            raw,
            callbackCStr.map { UnsafePointer($0) })
        else { return nil }
        defer { nmp_free_string(ptr) }
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
    ///
    /// Uses ADR-0063 Lane D `nmp_app_resolve_ref` (namespace=0/profile,
    /// shape=1/profile.card, liveness=1/Live for open-screen claims).
    func claimProfile(pubkeyHex: String, consumerID: String) {
        pubkeyHex.withCString { pk in
            consumerID.withCString { cid in
                nmp_app_resolve_ref(raw, 0, pk, cid, 1, 1)
            }
        }
    }

    /// Release a previously-claimed profile interest. The kernel drops the
    /// pending request when the last consumer releases. Mirrors `claimProfile`.
    ///
    /// Uses ADR-0063 Lane D `nmp_app_release_ref` (namespace=0/profile).
    func releaseProfile(pubkeyHex: String, consumerID: String) {
        pubkeyHex.withCString { pk in
            consumerID.withCString { cid in
                nmp_app_release_ref(raw, 0, pk, cid)
            }
        }
    }
}
