import Foundation

// MARK: - UserIdentityStore → Rust kernel identity wiring
//
// Bridges the user's Nostr identity into the Rust kernel's podcast-app
// `IdentityStore` (`apps/nmp-app-podcast/src/store/identity.rs`).
//
// ## Why this exists
//
// Before this wiring the kernel's podcast-app `IdentityStore` was populated
// ONLY by a `podcast.identity` `ImportNsec` dispatch — which iOS never sent.
// Users signed in through `UserIdentityStore`, which kept the key in the
// Swift Keychain and a Swift `NostrSigner` only. As a result every
// kernel-side feature that signs on behalf of the user (agent-to-agent
// notes, and now the `podcast.social` kind:0/1/9802 publishing) saw an empty
// store and failed with `"not signed in"`.
//
// This extension closes that gap: whenever a LOCAL key is adopted (import,
// generate, or launch-time load) we forward the key to the kernel via
// `podcast.identity` `ImportNsec`, mirroring the Android `MainActivity`
// `IdentityActions.importNsec` pattern. The kernel then owns the signing key
// for the duration of the session.
//
// ## Local key vs. remote signer (NIP-46 bunker)
//
// A local key has a private key Swift can hand to the kernel — so the kernel
// can sign locally. A NIP-46 *bunker* keeps the key remote by design; there
// is no private key to forward, so the kernel's podcast-app `IdentityStore`
// cannot sign for it. Bunker connections are wired through the kernel's
// dedicated signer-broker path (`KernelModel.signInBunker`) so the template
// identity surfaces the account, but `podcast.social` signing stays on the
// Swift NIP-46 path for `.remoteSigner` mode. See
// `UserIdentityStore+Publishing.swift` and BACKLOG
// `social-bunker-signing-kernel`.
//
// SECURITY: the value forwarded here is the user's private key (as hex). It
// is dispatched in-process to the kernel only; the kernel wraps it for its
// own persistence (`identity.json`). It is NEVER logged.

extension UserIdentityStore {

    /// Attach the kernel so identity changes propagate into the Rust store.
    /// Called once from `AppStateStore.attachKernel`. Immediately syncs the
    /// current identity so a key adopted before the kernel attached (the
    /// common launch ordering) still reaches the kernel.
    @MainActor
    func attachKernel(_ kernel: KernelModel) {
        self.kernel = kernel
        syncIdentityToKernel()
    }

    /// Forward the currently-active identity to the kernel.
    ///
    /// * `.localKey` — dispatch `podcast.identity` `ImportNsec` with the
    ///   private key hex so the kernel can sign locally.
    /// * `.remoteSigner` — the key is remote; nothing to import. The bunker
    ///   connection is wired separately via `KernelModel.signInBunker`.
    /// * `.none` — no-op.
    ///
    /// Idempotent: re-importing the same key is a cheap no-op in the kernel
    /// (it re-derives the same pubkey and rewrites `identity.json`).
    @MainActor
    func syncIdentityToKernel() {
        // A recorder (tests) intercepts even without a live kernel; production
        // requires the kernel ref.
        guard kernel != nil || _kernelDispatchRecorder != nil else { return }
        switch mode {
        case .localKey:
            guard let privateKeyHex = keyPair?.privateKeyHex else { return }
            // Silent — this is an internal identity sync, not a user-initiated
            // action; a transient rejection must not toast.
            dispatchToKernel(
                namespace: "podcast.identity",
                body: ["type": "ImportNsec", "nsec": privateKeyHex],
                silent: true
            )
        case .remoteSigner, .none:
            // Remote-signer keys never materialise in-process; the bunker is
            // wired through the kernel signer broker at connect time.
            break
        }
    }

    /// Wipe the active identity from the kernel's podcast-app `IdentityStore`
    /// (and delete its persisted `identity.json`). MUST be called on sign-out
    /// so the user's key does not outlive sign-out and remain able to sign
    /// kernel-side. Dispatches `podcast.identity` `Clear`.
    @MainActor
    func clearIdentityInKernel() {
        dispatchToKernel(
            namespace: "podcast.identity",
            body: ["type": "Clear"],
            silent: true
        )
    }

    /// Wire a NIP-46 bunker connection into the kernel's signer broker so
    /// kernel-side features that delegate signing over the relay can resolve
    /// the remote signer. The kernel owns persistence of the bunker session;
    /// this is a no-op if the broker was never initialised (silent per D6).
    ///
    /// Call after a successful `connectRemoteSigner` / nostrconnect pairing.
    @MainActor
    func syncBunkerToKernel(uri: String) {
        kernel?.signInBunker(uri: uri)
    }

    /// Route a kernel dispatch through the test recorder when present,
    /// otherwise to the real kernel. The single choke point every social /
    /// identity dispatch uses. `silent` picks `dispatchSilent` (internal
    /// syncs) over `dispatch` (user actions that may toast on rejection).
    @MainActor
    func dispatchToKernel(namespace: String, body: [String: Any], silent: Bool = false) {
        if let recorder = _kernelDispatchRecorder {
            recorder(namespace, body)
        } else if silent {
            kernel?.dispatchSilent(namespace: namespace, body: body)
        } else {
            kernel?.dispatch(namespace: namespace, body: body)
        }
    }

    /// Test-only: install a recorder that captures `podcast.social` /
    /// `podcast.identity` dispatches in place of the (unavailable) real
    /// kernel, so the wiring tests can assert the kernel signing path.
    func _setKernelRecorderForTesting(
        _ recorder: @escaping @MainActor (String, [String: Any]) -> Void
    ) {
        self._kernelDispatchRecorder = recorder
    }
}
