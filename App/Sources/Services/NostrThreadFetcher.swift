import Foundation
import os.log

/// One-shot WebSocket fetch for a NIP-10 conversation: the root event
/// itself plus every kind:1 that e-tags it. Used by the peer-agent
/// responder to assemble the message history before invoking the LLM.
///
/// Pattern mirrors `NostrProfileFetcher`: open a fresh socket, send a
/// single multi-filter REQ, read EVENTs until EOSE or a hard timeout,
/// then close. Each call owns its own socket and subscription id so
/// concurrent fetches don't collide.
@MainActor
final class NostrThreadFetcher {

    nonisolated private static let logger = Logger.app("NostrThreadFetcher")

    private enum Wire {
        static let kindTextNote = 1
        static let req = "REQ"
        static let close = "CLOSE"
        static let event = "EVENT"
        static let eose = "EOSE"
        static let timeout: Duration = .seconds(4)
    }

    /// Wire-shape of an inbound kind:1 the responder needs to assemble a
    /// conversation. Keeps the responder decoupled from raw `[String: Any]`
    /// dictionaries by surfacing only the fields it actually reads.
    struct Event: Sendable, Equatable {
        let id: String
        let pubkey: String
        let createdAt: Int
        let content: String
        let tags: [[String]]
    }

    /// Per-instance accumulator. Each `fetch` builds a fresh fetcher so
    /// concurrent fetches don't share state.
    private var collected: [String: Event] = [:]

    /// Fetch the root (by id) and all kind:1 replies that e-tag it from
    /// the configured relay. Results are de-duplicated by event id and
    /// sorted ascending by `created_at`. Returns an empty array on any
    /// hard failure — the caller is expected to proceed with whatever
    /// the inbound event itself carries.
    static func fetch(rootID: String, relayURL: URL) async -> [Event] {
        await NostrThreadFetcher().run(rootID: rootID, relayURL: relayURL)
    }

    private init() {}

    private func run(rootID: String, relayURL: URL) async -> [Event] {
        let task = URLSession.shared.webSocketTask(with: relayURL)
        task.resume()
        defer { task.cancel(with: .normalClosure, reason: nil) }

        let subID = "thread-\(UUID().uuidString.prefix(8))"
        // One REQ, two filters — pulls both the root and its replies in a
        // single subscription so we get a single EOSE that closes the
        // socket cleanly.
        let idFilter: [String: Any] = ["ids": [rootID]]
        let replyFilter: [String: Any] = ["kinds": [Wire.kindTextNote], "#e": [rootID]]
        let req: [Any] = [Wire.req, subID, idFilter, replyFilter]

        guard let payload = try? JSONSerialization.data(withJSONObject: req),
              let text = String(data: payload, encoding: .utf8) else {
            return []
        }

        do {
            try await task.send(.string(text))
        } catch {
            Self.logger.warning("fetch: send failed — \(error, privacy: .public)")
            return []
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

        return collected.values.sorted { $0.createdAt < $1.createdAt }
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
              let head = array[0] as? String else { return .other }
        switch head {
        case Wire.eose:
            return (array[1] as? String) == expectedSubID ? .eose : .other
        case Wire.event:
            guard array.count >= 3,
                  (array[1] as? String) == expectedSubID,
                  let dict = array[2] as? [String: Any],
                  let id = dict["id"] as? String,
                  let pubkey = dict["pubkey"] as? String,
                  let createdAt = dict["created_at"] as? Int,
                  let content = dict["content"] as? String else { return .other }
            let tags = (dict["tags"] as? [[String]]) ?? []
            collected[id] = Event(
                id: id,
                pubkey: pubkey,
                createdAt: createdAt,
                content: content,
                tags: tags
            )
            return .event
        default:
            return .other
        }
    }
}
