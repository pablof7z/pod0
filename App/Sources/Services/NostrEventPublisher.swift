import Foundation

// MARK: - NostrEventPublishing
//
// Shared one-shot WebSocket publisher used by lightweight Nostr writers that
// don't want to take a long-lived `NostrRelayService` dependency (e.g. the
// agent peer-tools surface). Opens a transient socket, sends a single EVENT
// frame, validates the OK ack, then closes.

protocol NostrEventPublishing: Sendable {
    func publish(event: SignedNostrEvent, relayURL: URL) async throws
}

struct NostrWebSocketEventPublisher: NostrEventPublishing {
    func publish(event: SignedNostrEvent, relayURL: URL) async throws {
        let task = URLSession.shared.webSocketTask(with: relayURL)
        task.resume()
        defer { task.cancel(with: .normalClosure, reason: nil) }

        let message: [Any] = ["EVENT", eventDictionary(event)]
        let data = try JSONSerialization.data(withJSONObject: message)
        guard let text = String(data: data, encoding: .utf8) else {
            throw NostrEventPublisherError.encodingFailed
        }
        try await send(text, task: task)
        try await validateRelayAck(for: event.id, task: task)
    }

    private func send(_ text: String, task: URLSessionWebSocketTask) async throws {
        let _: Void = try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, any Error>) in
            task.send(.string(text)) { error in
                if let error {
                    continuation.resume(throwing: error)
                } else {
                    continuation.resume()
                }
            }
        }
    }

    private func validateRelayAck(for eventID: String, task: URLSessionWebSocketTask) async throws {
        let message = try await task.receive()
        guard case .string(let text) = message,
              let data = text.data(using: .utf8),
              let array = try JSONSerialization.jsonObject(with: data) as? [Any],
              array.count >= 3,
              array[0] as? String == "OK",
              array[1] as? String == eventID else {
            throw NostrEventPublisherError.missingOK
        }
        if let accepted = array[2] as? Bool, !accepted {
            let reason = array.count > 3 ? (array[3] as? String) : nil
            throw NostrEventPublisherError.rejected(reason ?? "Relay rejected event.")
        }
    }

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

enum NostrEventPublisherError: LocalizedError {
    case encodingFailed
    case missingOK
    case rejected(String)
    case noRelayConfigured
    case noSigningKey

    var errorDescription: String? {
        switch self {
        case .encodingFailed: return "Could not encode Nostr EVENT message."
        case .missingOK: return "Relay did not acknowledge the event."
        case .rejected(let reason): return reason
        case .noRelayConfigured: return "No Nostr relay is configured."
        case .noSigningKey: return "No Nostr signing key is available."
        }
    }
}
