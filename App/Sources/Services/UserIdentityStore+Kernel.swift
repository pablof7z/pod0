import Foundation

// MARK: - UserIdentityStore → Rust kernel identity wiring
//
// The Rust kernel owns ALL key material and signing. This extension wires the
// Swift identity store to the kernel for the actions Swift still initiates:
//   * social publishing dispatches (`podcast.social.*`) — the kernel signs
//     with the active account (local key OR NIP-46 bunker).
//   * sign-out (`podcast.identity` `Clear`) — wipe the active key from the
//     kernel store + its persisted `identity.json`.
//   * bunker pairing (`signInBunker`) — hand the broker a `bunker://` URI.
//
// There is NO key forwarding from Swift to the kernel anymore: the kernel
// generates / imports / persists keys itself (`podcast.identity`) and mirrors
// local keys into NMP's active signer roster; Swift never holds or transmits
// private bytes.

extension UserIdentityStore {

    /// Attach the kernel so identity actions reach the Rust store. Called once
    /// from `AppStateStore.attachKernel`. The kernel restores its own identity
    /// asynchronously; the active pubkey arrives via `applyKernelIdentity` on
    /// the first snapshot tick (no eager Swift-side sync needed).
    @MainActor
    func attachKernel(_ kernel: KernelModel) {
        self.kernel = kernel
    }

    /// Wipe the active identity from the kernel's podcast-app identity store
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
    @MainActor
    func syncBunkerToKernel(uri: String) {
        kernel?.signInBunker(uri: uri)
    }

    /// Route a kernel dispatch through the test recorder when present,
    /// otherwise to the real kernel. The single choke point every social /
    /// identity dispatch uses. `silent` picks `dispatchSilent` (internal
    /// syncs) over `dispatch` (user actions that may toast on rejection).
    ///
    /// Returns the synchronous `DispatchResult` so callers that care about
    /// synchronous rejection (e.g. `publishProfile`) can surface it as an
    /// error. Mark `@discardableResult` so existing callers that ignore the
    /// result (sign-out, keygen, importNsec) are unaffected.
    ///
    /// - Test-recorder path: returns `.accepted(correlationId: "")` — the
    ///   recorder captures the dispatch for assertion; there is no kernel to
    ///   reject, so treating it as accepted keeps tests on the success path.
    /// - Silent path (internal best-effort syncs): returns whatever the kernel
    ///   returns, or `.accepted(correlationId: "")` if no kernel is attached.
    ///   Nil kernel on the silent path is not treated as failure because
    ///   silent dispatches are best-effort and callers do not check results.
    /// - Normal path: returns the kernel's result, or
    ///   `.failure("no active kernel")` when no kernel is attached —
    ///   which is a genuine synchronous rejection.
    @MainActor
    @discardableResult
    func dispatchToKernel(namespace: String, body: [String: Any], silent: Bool = false) -> DispatchResult {
        if let recorder = _kernelDispatchRecorder {
            recorder(namespace, body)
            return .accepted(correlationId: "")
        } else if silent {
            return kernel?.dispatchSilent(namespace: namespace, body: body)
                ?? .accepted(correlationId: "")
        } else {
            return kernel?.dispatch(namespace: namespace, body: body)
                ?? .failure("no active kernel")
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
