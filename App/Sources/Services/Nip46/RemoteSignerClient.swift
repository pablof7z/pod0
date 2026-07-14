import CryptoKit
import Foundation
import os.log
import P256K

/// Owns the WebSocket relay connection that ferries NIP-46 JSON-RPC traffic between
/// us and a bunker. Pure transport: encryption / event signing live in `RemoteSigner`.
///
/// We open the relay, send a `REQ` filter for kind:24133 events tagged with our session
/// pubkey, and route incoming events back through the `onEvent` callback.
actor RemoteSignerClient: RemoteSignerTransport {
    private static let logger = Logger.app("RemoteSignerClient")

    private let relayURL: URL
    private let sessionPubkeyHex: String
    /// Nil during the nostrconnect pairing phase — omits the `authors` filter so we
    /// accept the initial connect response from an as-yet-unknown signer pubkey.
    private let bunkerPubkeyHex: String?
    /// Called for every kind:24133 event whose `#p` matches our session pubkey. Includes
    /// the **encrypted** content (the caller is responsible for NIP-44 decryption).
    private let onEvent: @Sendable (_ senderPubkey: String, _ encryptedContent: String) async -> Void

    private var task: URLSessionWebSocketTask?
    private var receiveLoop: Task<Void, Never>?
    private(set) var isConnected = false

    private enum Constants {
        static let kind = 24133
        static let subscriptionID = "nip46-inbox"
        /// Send retry budget when the socket disappears mid-publish.
        static let sendRetries = 1
    }

    init(
        relayURL: URL,
        sessionPubkeyHex: String,
        bunkerPubkeyHex: String?,
        onEvent: @escaping @Sendable (_ senderPubkey: String, _ encryptedContent: String) async -> Void
    ) {
        self.relayURL = relayURL
        self.sessionPubkeyHex = sessionPubkeyHex
        self.bunkerPubkeyHex = bunkerPubkeyHex
        self.onEvent = onEvent
    }

    func connect() {
        guard task == nil else { return }
        let t = URLSession.shared.webSocketTask(with: relayURL)
        task = t
        t.resume()
        isConnected = true
        sendSubscription()
        startReceiveLoop()
        Self.logger.info("RemoteSignerClient: connected to \(self.relayURL.absoluteString, privacy: .public)")
    }

    func disconnect() {
        receiveLoop?.cancel()
        receiveLoop = nil
        task?.cancel(with: .goingAway, reason: nil)
        task = nil
        isConnected = false
    }

    // MARK: - Publish a kind:24133 event

    /// Send the already-built (signed + encrypted) event over the wire as a NIP-01 EVENT message.
    func publish(event: SignedNostrEvent) async throws {
        guard let task else { throw NostrSignerError.notConnected }
        let message: [Any] = ["EVENT", eventDictionary(event)]
        let data = try JSONSerialization.data(withJSONObject: message, options: [])
        guard let text = String(data: data, encoding: .utf8) else {
            throw NostrSignerError.invalidEventForSigning
        }
        try await task.send(.string(text))
    }

    // MARK: - REQ subscription

    private func sendSubscription() {
        var filter: [String: Any] = [
            "kinds": [Constants.kind],
            "#p": [sessionPubkeyHex],
        ]
        if let bunkerPubkeyHex {
            filter["authors"] = [bunkerPubkeyHex]
        }
        let message: [Any] = ["REQ", Constants.subscriptionID, filter]
        guard let data = try? JSONSerialization.data(withJSONObject: message, options: []),
              let text = String(data: data, encoding: .utf8) else { return }
        task?.send(.string(text)) { error in
            if let error {
                Self.logger.error("RemoteSignerClient: REQ send failed — \(error, privacy: .public)")
            }
        }
    }

    // MARK: - Receive loop

    private func startReceiveLoop() {
        receiveLoop = Task { [weak self] in
            guard let self else { return }
            while !Task.isCancelled {
                guard let t = await self.task else { return }
                do {
                    let msg = try await t.receive()
                    if case .string(let text) = msg {
                        await self.handle(text: text)
                    }
                } catch {
                    if !Task.isCancelled {
                        Self.logger.warning("RemoteSignerClient: WS error — \(error, privacy: .public)")
                    }
                    return
                }
            }
        }
    }

    private func handle(text: String) async {
        guard let data = text.data(using: .utf8),
              let arr = try? JSONSerialization.jsonObject(with: data) as? [Any],
              let head = arr.first as? String else { return }
        switch head {
        case "EVENT":
            // ["EVENT", "<sub_id>", { event-object }]
            guard arr.count >= 3, let event = arr[2] as? [String: Any],
                  let kind = event["kind"] as? Int, kind == Constants.kind,
                  let sender = event["pubkey"] as? String,
                  let content = event["content"] as? String else { return }
            await onEvent(sender, content)
        case "OK":
            // ["OK", "<event_id>", true/false, "<message>"] — log & continue
            if arr.count >= 4, let accepted = arr[2] as? Bool, !accepted {
                Self.logger.warning("RemoteSignerClient: relay rejected event — \(String(describing: arr[3]), privacy: .public)")
            }
        case "EOSE", "NOTICE", "CLOSED":
            return
        default:
            return
        }
    }

    // MARK: - Helpers

    private func eventDictionary(_ event: SignedNostrEvent) -> [String: Any] {
        [
            "id": event.id,
            "pubkey": event.pubkey,
            "created_at": event.created_at,
            "kind": event.kind,
            "tags": event.tags,
            "content": event.content,
            "sig": event.sig,
        ]
    }
}
