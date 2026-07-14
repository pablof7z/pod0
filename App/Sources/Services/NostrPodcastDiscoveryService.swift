import CryptoKit
import Foundation
import os.log

// MARK: - NostrPodcastDiscoveryService
//
// Queries a Nostr relay for NIP-74 podcast events:
//   kind:30074 — podcast show (parameterised replaceable, d-tag = show identifier)
//   kind:30075 — podcast episode (parameterised replaceable, d-tag = episode identifier)
//
// Each query opens a short-lived WebSocket, collects events until EOSE or a
// hard timeout, then closes. Models `NostrProfileFetcher` for the socket lifecycle.

@MainActor
final class NostrPodcastDiscoveryService {

    nonisolated private static let logger = Logger.app("NostrPodcastDiscoveryService")

    private enum Wire {
        static let kindShow = 30074
        static let kindEpisode = 30075
        static let req = "REQ"
        static let close = "CLOSE"
        static let event = "EVENT"
        static let eose = "EOSE"
        static let timeout: Duration = .seconds(8)
    }

    // MARK: - Public result types

    struct ShowResult: Identifiable, Sendable {
        /// Unique key: "<pubkey>:<dTag>"
        var id: String { "\(pubkey):\(dTag)" }
        let coordinate: String   // "30074:<pubkey>:<dTag>"
        let pubkey: String
        let dTag: String
        let title: String
        let author: String
        let imageURL: URL?
        let description: String
        let categories: [String]
        let createdAt: Int
    }

    // MARK: - Instance accumulator (main-actor isolated)

    private var collectedEvents: [[String: Any]] = []

    // MARK: - Fetch shows (kind:30074)

    /// Returns all kind:30074 shows the relay knows about, newest first.
    func fetchShows(relayURL: URL) async -> [ShowResult] {
        let subID = "nip74-shows-\(UUID().uuidString.prefix(8))"
        let filter: [String: Any] = ["kinds": [Wire.kindShow]]
        await collectEvents(relayURL: relayURL, subID: subID, filter: filter)

        var seen = Set<String>()
        return collectedEvents
            .compactMap { Self.parseShow(from: $0) }
            .sorted { $0.createdAt > $1.createdAt }
            .filter { seen.insert($0.coordinate).inserted }
    }

    // MARK: - Fetch episodes (kind:30075)

    /// Returns `Episode` objects for `show`, already mapped with `podcastID`.
    func fetchEpisodes(for show: ShowResult, relayURL: URL, podcastID: UUID) async -> [Episode] {
        let subID = "nip74-eps-\(UUID().uuidString.prefix(8))"
        let showRef = "\(Wire.kindShow):\(show.pubkey):\(show.dTag)"
        let filter: [String: Any] = [
            "kinds": [Wire.kindEpisode],
            "authors": [show.pubkey],
            "#a": [showRef],
        ]
        await collectEvents(relayURL: relayURL, subID: subID, filter: filter)

        // Dedupe by d-tag (replaceable events): keep the newest per d-tag.
        var seen = [String: Int]()
        var deduped: [[String: Any]] = []
        for event in collectedEvents.sorted(by: {
            ($0["created_at"] as? Int ?? 0) > ($1["created_at"] as? Int ?? 0)
        }) {
            let tags = (event["tags"] as? [[String]]) ?? []
            let dTag = tags.first { $0.first == "d" }?[safe: 1] ?? ""
            guard !dTag.isEmpty, seen[dTag] == nil else { continue }
            seen[dTag] = event["created_at"] as? Int ?? 0
            deduped.append(event)
        }
        return deduped.compactMap { Self.parseEpisode(from: $0, podcastID: podcastID) }
    }

    // MARK: - Deterministic UUID

    /// Derives a stable `UUID` from a NIP-74 coordinate using SHA-256.
    /// Identical coordinates always produce the same UUID, enabling dedup
    /// by `store.podcast(id:)` without a feedURL.
    static func podcastID(for coordinate: String) -> UUID {
        let hash = SHA256.hash(data: Data(coordinate.utf8))
        var bytes = Array(hash.prefix(16))
        bytes[6] = (bytes[6] & 0x0F) | 0x50  // UUID version 5
        bytes[8] = (bytes[8] & 0x3F) | 0x80  // UUID variant 1
        return UUID(uuid: (
            bytes[0], bytes[1], bytes[2], bytes[3],
            bytes[4], bytes[5], bytes[6], bytes[7],
            bytes[8], bytes[9], bytes[10], bytes[11],
            bytes[12], bytes[13], bytes[14], bytes[15]
        ))
    }

    // MARK: - Subscribe

    /// Creates or updates the `Podcast` row, upserts episodes, and adds a
    /// `PodcastSubscription`. Returns the stored podcast.
    func subscribe(to show: ShowResult, store: AppStateStore, relayURL: URL) async -> Podcast {
        let pid = Self.podcastID(for: show.coordinate)
        let draft = Podcast(
            id: pid,
            kind: .rss,
            feedURL: nil,
            title: show.title,
            author: show.author,
            imageURL: show.imageURL,
            description: show.description,
            categories: show.categories,
            nostrCoordinate: show.coordinate
        )
        let stored = store.upsertPodcast(draft)
        let episodes = await fetchEpisodes(for: show, relayURL: relayURL, podcastID: stored.id)
        store.upsertEpisodes(episodes, forPodcast: stored.id, evaluateAutoDownload: false)
        store.addSubscription(podcastID: stored.id)
        return stored
    }

    // MARK: - WebSocket collector

    /// Opens a short-lived WebSocket to `relayURL`, sends REQ filter, collects
    /// EVENT payloads until EOSE or the hard timeout, then closes.
    /// Results land in `collectedEvents` (cleared on each call).
    private func collectEvents(relayURL: URL, subID: String, filter: [String: Any]) async {
        collectedEvents = []

        let wsTask = URLSession.shared.webSocketTask(with: relayURL)
        wsTask.resume()

        let req: [Any] = [Wire.req, subID, filter]
        guard let payload = try? JSONSerialization.data(withJSONObject: req),
              let text = String(data: payload, encoding: .utf8) else {
            wsTask.cancel(with: .normalClosure, reason: nil)
            return
        }

        do {
            try await wsTask.send(.string(text))
        } catch {
            Self.logger.warning("collectEvents: REQ send failed — \(error, privacy: .public)")
            wsTask.cancel(with: .normalClosure, reason: nil)
            return
        }

        await withTaskGroup(of: Void.self) { [weak self] group in
            group.addTask { [weak self] in
                await self?.readUntilEose(task: wsTask, subID: subID)
            }
            group.addTask {
                try? await Task.sleep(for: Wire.timeout)
            }
            await group.next()
            group.cancelAll()
        }

        let close: [Any] = [Wire.close, subID]
        if let closeData = try? JSONSerialization.data(withJSONObject: close),
           let closeText = String(data: closeData, encoding: .utf8) {
            try? await wsTask.send(.string(closeText))
        }
        wsTask.cancel(with: .normalClosure, reason: nil)
    }

    private func readUntilEose(task: URLSessionWebSocketTask, subID: String) async {
        while !Task.isCancelled {
            do {
                let msg = try await task.receive()
                guard case .string(let text) = msg else { continue }
                guard let data = text.data(using: .utf8),
                      let array = try? JSONSerialization.jsonObject(with: data) as? [Any],
                      array.count >= 2,
                      let msgType = array[0] as? String else { continue }

                switch msgType {
                case Wire.eose:
                    if (array[1] as? String) == subID { return }
                case Wire.event:
                    guard array.count >= 3,
                          (array[1] as? String) == subID,
                          let event = array[2] as? [String: Any] else { continue }
                    collectedEvents.append(event)
                default:
                    break
                }
            } catch {
                return
            }
        }
    }

    // MARK: - Event parsers

    private static func parseShow(from event: [String: Any]) -> ShowResult? {
        guard let pubkey = event["pubkey"] as? String,
              let createdAt = event["created_at"] as? Int else { return nil }

        let tags = (event["tags"] as? [[String]]) ?? []
        guard let dTag = tags.first(where: { $0.first == "d" })?[safe: 1],
              !dTag.isEmpty else { return nil }

        let title = tags.first(where: { $0.first == "title" })?[safe: 1]
            ?? (event["content"] as? String).map { String($0.prefix(80)) }
            ?? ""
        guard !title.isEmpty else { return nil }

        let author = tags.first(where: { $0.first == "author" })?[safe: 1] ?? ""
        let description = tags.first(where: { $0.first == "summary" })?[safe: 1]
            ?? (event["content"] as? String) ?? ""
        let imageURL = tags.first(where: { $0.first == "image" })?[safe: 1]
            .flatMap { URL(string: $0) }
        let categories = tags.filter { $0.first == "t" }.compactMap { $0[safe: 1] }
        let coordinate = "\(Wire.kindShow):\(pubkey):\(dTag)"

        return ShowResult(
            coordinate: coordinate,
            pubkey: pubkey,
            dTag: dTag,
            title: title,
            author: author,
            imageURL: imageURL,
            description: description,
            categories: categories,
            createdAt: createdAt
        )
    }

    private static func parseEpisode(from event: [String: Any], podcastID: UUID) -> Episode? {
        let tags = (event["tags"] as? [[String]]) ?? []
        guard let dTag = tags.first(where: { $0.first == "d" })?[safe: 1],
              !dTag.isEmpty else { return nil }

        // Audio URL from `imeta url` or fallback `url` tag — required.
        let imetaTag = tags.first(where: { $0.first == "imeta" })
        let audioURLStr = imetaTag?
            .dropFirst()
            .compactMap { $0.hasPrefix("url ") ? String($0.dropFirst(4)) : nil }
            .first
            ?? tags.first(where: { $0.first == "url" })?[safe: 1]
        guard let audioStr = audioURLStr, let audioURL = URL(string: audioStr) else { return nil }

        let title = tags.first(where: { $0.first == "title" })?[safe: 1] ?? ""
        let description = tags.first(where: { $0.first == "summary" })?[safe: 1]
            ?? (event["content"] as? String) ?? ""
        let imageURL = tags.first(where: { $0.first == "image" })?[safe: 1]
            .flatMap { URL(string: $0) }

        let pubDateSeconds = tags.first(where: { $0.first == "published_at" })?[safe: 1]
            .flatMap { Int($0) } ?? (event["created_at"] as? Int) ?? 0
        let pubDate = Date(timeIntervalSince1970: TimeInterval(pubDateSeconds))

        let duration = tags.first(where: { $0.first == "duration" })?[safe: 1]
            .flatMap { TimeInterval($0) }

        let chaptersURL = tags.first(where: { $0.first == "chapters" })?[safe: 1]
            .flatMap { URL(string: $0) }

        let transcriptURL = tags.first(where: { $0.first == "transcript" })?[safe: 1]
            .flatMap { URL(string: $0) }
        let transcriptKind = TranscriptKind.from(
            mimeType: tags.first(where: { $0.first == "transcript" })?[safe: 2]
        )
        let mimeType = imetaTag?
            .dropFirst()
            .compactMap { $0.hasPrefix("m ") ? String($0.dropFirst(2)) : nil }
            .first

        return Episode(
            podcastID: podcastID,
            guid: dTag,
            title: title.isEmpty ? "Untitled Episode" : title,
            description: description,
            pubDate: pubDate,
            duration: duration,
            enclosureURL: audioURL,
            enclosureMimeType: mimeType,
            imageURL: imageURL,
            publisherTranscriptURL: transcriptURL,
            publisherTranscriptType: transcriptKind,
            chaptersURL: chaptersURL
        )
    }
}

// MARK: - Array safe subscript

private extension Array {
    subscript(safe index: Int) -> Element? {
        indices.contains(index) ? self[index] : nil
    }
}
