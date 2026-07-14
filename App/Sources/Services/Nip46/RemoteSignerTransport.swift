import Foundation

/// Transport surface used by `RemoteSigner` to talk to a single relay. Pulled out as a
/// protocol so tests can inject a mock that scripts inbound NIP-46 frames without ever
/// opening a real WebSocket. `RemoteSignerClient` is the production conformer.
protocol RemoteSignerTransport: Sendable, AnyObject {
    /// Open the relay connection and start the receive loop. Must be safe to call once.
    func connect() async
    /// Tear down the connection. Idempotent.
    func disconnect() async
    /// Publish a fully-signed kind:24133 NIP-46 event.
    func publish(event: SignedNostrEvent) async throws
}

/// Factory that produces a `RemoteSignerTransport` for a given relay URL. Injected into
/// `RemoteSigner` so production code uses the real WebSocket client and tests use a mock.
///
/// Parameters mirror `RemoteSignerClient.init`:
/// - `relayURL`: which relay to dial.
/// - `sessionPubkeyHex`: our ephemeral session pubkey (used for `#p` filter).
/// - `bunkerPubkeyHex`: bunker's pubkey (used for `authors` filter). Nil during nostrconnect
///   pairing — omits the `authors` filter so the initial connect response is accepted.
/// - `onEvent`: callback fired for every kind:24133 event addressed to us — `senderPubkey`
///   is the event author, `encryptedContent` is the still-NIP-44-encrypted `content` field.
typealias RemoteSignerTransportFactory = @Sendable (
    _ relayURL: URL,
    _ sessionPubkeyHex: String,
    _ bunkerPubkeyHex: String?,
    _ onEvent: @escaping @Sendable (_ senderPubkey: String, _ encryptedContent: String) async -> Void
) -> any RemoteSignerTransport

/// Default factory — wraps the real `RemoteSignerClient`.
let defaultRemoteSignerTransportFactory: RemoteSignerTransportFactory = { url, session, bunker, onEvent in
    RemoteSignerClient(
        relayURL: url,
        sessionPubkeyHex: session,
        bunkerPubkeyHex: bunker,
        onEvent: onEvent
    )
}
