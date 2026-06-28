import Foundation

// MARK: - UserIdentityStore nostrconnect flow
//
// Routes all NIP-46 pairing through the NMP kernel — no Swift WebSocket,
// no key material, no remote-signer crypto in this path.
//
// * `connectViaNostrConnect`: the kernel generates the URI AND starts
//   listening; the UI observes `remoteSignerState` reactively via
//   `applyKernelIdentity` (in `UserIdentityStore.swift`), which maps the
//   kernel's handshake + active account onto published state on every
//   snapshot tick.

extension UserIdentityStore {

    /// Begin nostrconnect:// pairing via the kernel's NIP-46 broker.
    /// The kernel selects a configured write relay, generates the URI, and
    /// immediately starts listening for the signer app's response — no Swift
    /// WebSocket.
    /// State changes arrive via `applyKernelIdentity` on the next snapshot tick.
    func connectViaNostrConnect(
        relay: URL? = nil,
        onURI: @escaping @Sendable (String) -> Void
    ) async {
        _beginNostrConnect()
        guard let uri = kernel?.nostrconnectURI(
            relayURL: nil,
            callbackScheme: "podcastr"
        ) else {
            _failNostrConnect("Kernel unavailable")
            return
        }
        onURI(uri)
        // State progression (connecting → connected/failed) comes reactively
        // via AppStateStore → applyKernelIdentity on kernel snapshot ticks.
    }
}
