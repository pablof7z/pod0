import Foundation

#if canImport(NMP)
extension UserIdentityStore {
    /// Client-initiated NIP-46 remains fail-closed until upstream NMP #571
    /// exposes a secure checkpoint and cold-start restore surface. This path
    /// never falls back to Pod0's retired RemoteSigner transport.
    func connectViaNostrConnect(
        relay _: URL = URL(string: "wss://relay.nsec.app")!,
        onURI _: @escaping @Sendable (String) -> Void
    ) async {
        remoteSignerState = .connecting
        do {
            guard let lifecycle = nmpLifecycle else {
                throw UserIdentityError.nmpUnavailable
            }
            try lifecycle.connectClientInitiated(relays: [])
        } catch {
            failIdentity(error)
        }
    }
}
#endif
