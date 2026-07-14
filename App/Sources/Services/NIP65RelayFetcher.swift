import Foundation
import os.log

// MARK: - NIP65RelayFetcher
//
// Fetches the user's kind:10002 relay list (NIP-65 outbox model) so that
// NIP-74 podcast events can be published to the user's own write relays.
//
// Bootstrap problem: we query a set of well-known relays (primal + damus)
// plus the configured inbox relay. Write relays are `["r", url]` tags
// without a marker or with `"write"` marker; read-only tags (`"read"`) are
// excluded.

struct NIP65RelayFetcher: Sendable {

    static let logger = Logger.app("NIP65RelayFetcher")

    static let defaultRelays: [String] = [
        "wss://relay.primal.net",
        "wss://relay.damus.io",
    ]

    private static let bootstrapRelays: [String] = [
        "wss://relay.primal.net",
        "wss://relay.damus.io",
    ]

    private static let timeout: Duration = .seconds(5)
    private static let kind10002 = 10002

    /// Fetches kind:10002 write relays for `pubkeyHex`.
    /// Queries bootstrap relays plus `extraRelayURL` (the configured inbox relay).
    /// Returns the write-relay URLs, or an empty array if nothing was found.
    static func fetchWriteRelays(for pubkeyHex: String, extraRelayURL: String?) async -> [String] {
        var candidateRelays = bootstrapRelays
        if let extra = extraRelayURL, !extra.isEmpty, !candidateRelays.contains(extra) {
            candidateRelays.append(extra)
        }

        // Query all bootstrap relays in parallel; take the freshest kind:10002 event.
        var bestEvent: (createdAt: Int, tags: [[String]])?
        await withTaskGroup(of: (Int, [[String]])?.self) { group in
            for relayURL in candidateRelays {
                group.addTask {
                    await fetchKind10002(from: relayURL, pubkeyHex: pubkeyHex)
                }
            }
            for await result in group {
                guard let (createdAt, tags) = result else { continue }
                if bestEvent == nil || createdAt > bestEvent!.createdAt {
                    bestEvent = (createdAt, tags)
                }
            }
        }

        guard let (_, tags) = bestEvent else { return [] }
        return parseWriteRelays(from: tags)
    }

    // MARK: - Private

    private static func fetchKind10002(
        from relayURLString: String,
        pubkeyHex: String
    ) async -> (Int, [[String]])? {
        guard let url = URL(string: relayURLString) else { return nil }

        let task = URLSession.shared.webSocketTask(with: url)
        task.resume()
        defer { task.cancel(with: .normalClosure, reason: nil) }

        let subID = "nip65-\(UUID().uuidString.prefix(8))"
        let filter: [String: Any] = [
            "kinds": [kind10002],
            "authors": [pubkeyHex],
            "limit": 1,
        ]
        guard let payload = try? JSONSerialization.data(withJSONObject: ["REQ", subID, filter]),
              let text = String(data: payload, encoding: .utf8) else { return nil }

        do { try await task.send(.string(text)) } catch { return nil }

        return await withTaskGroup(of: (Int, [[String]])?.self) { group in
            group.addTask { await readEvent(task: task, subID: subID) }
            group.addTask {
                try? await Task.sleep(for: timeout)
                return nil
            }
            let result = await group.next() ?? nil
            group.cancelAll()
            return result
        }
    }

    private static func readEvent(
        task: URLSessionWebSocketTask,
        subID: String
    ) async -> (Int, [[String]])? {
        while !Task.isCancelled {
            guard let msg = try? await task.receive(),
                  case .string(let text) = msg,
                  let data = text.data(using: .utf8),
                  let array = try? JSONSerialization.jsonObject(with: data) as? [Any],
                  let msgType = array.first as? String else { continue }

            if msgType == "EOSE" { return nil }

            guard msgType == "EVENT",
                  array.count >= 3,
                  (array[1] as? String) == subID,
                  let event = array[2] as? [String: Any],
                  (event["kind"] as? Int) == kind10002,
                  let createdAt = event["created_at"] as? Int,
                  let tags = event["tags"] as? [[String]] else { continue }

            return (createdAt, tags)
        }
        return nil
    }

    /// Extracts write relay URLs from kind:10002 `r` tags.
    /// Includes tags with no marker (both read+write) and tags with "write" marker.
    /// Excludes "read"-only tags.
    static func parseWriteRelays(from tags: [[String]]) -> [String] {
        tags.compactMap { tag -> String? in
            guard tag.count >= 2, tag[0] == "r" else { return nil }
            let url = tag[1]
            guard !url.isEmpty else { return nil }
            if tag.count >= 3 {
                // Has a marker — only include "write", skip "read"
                return tag[2] == "write" ? url : nil
            }
            // No marker = both read and write
            return url
        }
    }
}
