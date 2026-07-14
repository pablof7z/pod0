import Foundation
import os.log

/// One-shot kind:0 (`metadata`) fetcher. Opens a short-lived websocket to
/// the configured relay, requests profile events for the given pubkeys,
/// parses each event's JSON content, and writes the freshest record per
/// pubkey into `AppStateStore.state.nostrProfileCache`.
///
/// Designed to be cheap to call: the websocket is closed as soon as EOSE
/// arrives, or after a hard timeout. Concurrent calls are safe — each one
/// owns its own socket and `REQ` id.
@MainActor
final class NostrProfileFetcher {

    nonisolated private static let logger = Logger.app("NostrProfileFetcher")

    private enum Wire {
        static let kindMetadata = 0
        static let req = "REQ"
        static let close = "CLOSE"
        static let event = "EVENT"
        static let eose = "EOSE"
        static let timeout: Duration = .seconds(4)
    }

    private let store: AppStateStore

    init(store: AppStateStore) {
        self.store = store
    }

    /// Requests kind:0 events for `pubkeys` and caches whatever comes back
    /// before EOSE or timeout. Returns when the socket closes.
    func fetchProfiles(for pubkeys: [String]) async {
        guard !pubkeys.isEmpty else { return }
        let relayURL = store.state.settings.nostrRelayURL
        guard !relayURL.isEmpty, let url = URL(string: relayURL) else { return }

        let task = URLSession.shared.webSocketTask(with: url)
        task.resume()

        let subID = "profile-\(UUID().uuidString.prefix(8))"
        let filter: [String: Any] = [
            "kinds": [Wire.kindMetadata],
            "authors": pubkeys,
        ]
        let req: [Any] = [Wire.req, subID, filter]
        guard let payload = try? JSONSerialization.data(withJSONObject: req),
              let text = String(data: payload, encoding: .utf8) else {
            task.cancel(with: .normalClosure, reason: nil)
            return
        }

        do {
            try await task.send(.string(text))
        } catch {
            Self.logger.warning("fetchProfiles: send failed — \(error, privacy: .public)")
            task.cancel(with: .normalClosure, reason: nil)
            return
        }

        await withTaskGroup(of: Void.self) { group in
            group.addTask { [weak self] in
                await self?.readUntilEose(task: task, subID: subID)
            }
            group.addTask {
                try? await Task.sleep(for: Wire.timeout)
            }
            await group.next()
            group.cancelAll()
        }

        let close: [Any] = [Wire.close, subID]
        if let data = try? JSONSerialization.data(withJSONObject: close),
           let str = String(data: data, encoding: .utf8) {
            try? await task.send(.string(str))
        }
        task.cancel(with: .normalClosure, reason: nil)
    }

    private func readUntilEose(task: URLSessionWebSocketTask, subID: String) async {
        while !Task.isCancelled {
            do {
                let msg = try await task.receive()
                guard case .string(let text) = msg else { continue }
                if handleMessage(text: text, expectedSubID: subID) == .eose {
                    return
                }
            } catch {
                return
            }
        }
    }

    private enum HandleOutcome { case event, eose, other }

    @discardableResult
    private func handleMessage(text: String, expectedSubID: String) -> HandleOutcome {
        guard let data = text.data(using: .utf8),
              let array = try? JSONSerialization.jsonObject(with: data) as? [Any],
              array.count >= 2,
              let kind = array[0] as? String else { return .other }
        switch kind {
        case Wire.eose:
            return array.count >= 2 && (array[1] as? String) == expectedSubID ? .eose : .other
        case Wire.event:
            guard array.count >= 3,
                  (array[1] as? String) == expectedSubID,
                  let event = array[2] as? [String: Any] else { return .other }
            if let profile = parseProfile(from: event) {
                store.setNostrProfile(profile)
            }
            return .event
        default:
            return .other
        }
    }

    private func parseProfile(from event: [String: Any]) -> NostrProfileMetadata? {
        guard let pubkey = event["pubkey"] as? String,
              let createdAt = event["created_at"] as? Int,
              let content = event["content"] as? String else { return nil }

        guard let contentData = content.data(using: .utf8),
              let json = try? JSONSerialization.jsonObject(with: contentData) as? [String: Any] else {
            return NostrProfileMetadata(
                pubkey: pubkey,
                name: nil, displayName: nil, about: nil, picture: nil, nip05: nil,
                fetchedFromCreatedAt: createdAt
            )
        }

        return NostrProfileMetadata(
            pubkey: pubkey,
            name: json["name"] as? String,
            displayName: (json["display_name"] as? String) ?? (json["displayName"] as? String),
            about: json["about"] as? String,
            picture: json["picture"] as? String,
            nip05: json["nip05"] as? String,
            fetchedFromCreatedAt: createdAt
        )
    }
}
