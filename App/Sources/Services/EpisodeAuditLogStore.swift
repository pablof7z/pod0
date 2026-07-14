import Foundation
import os.log

// MARK: - EpisodeAuditLogStore

/// Append-only per-episode audit log persisted as JSON.
///
/// Files live under `$applicationSupport/podcastr/audit/<episodeID>.json` —
/// same `Application Support` shape as `TranscriptStore` and
/// `EpisodeDownloadStore` so a single fallback to `temporaryDirectory` covers
/// every persistence path when the container is unavailable.
///
/// Concurrency: every mutation runs on `@MainActor`. Writes are synchronous —
/// the volume of events per episode is small (tens), and a synchronous append
/// gives us a clean ordering guarantee without an extra lock. Reads from
/// background contexts go through `loadDetached(episodeID:)` which performs
/// its own JSON decode on the caller's queue.
///
/// Cap: the most recent `maxEventsPerEpisode` entries are retained. This is
/// generous (a transcript ingest produces ~6 events, a download ~3, plus
/// retries) so the cap really only kicks in for episodes the user repeatedly
/// retries by hand.
@MainActor
@Observable
final class EpisodeAuditLogStore {

    // MARK: Singleton

    static let shared = EpisodeAuditLogStore()

    // MARK: Logger

    nonisolated private static let logger = Logger.app("EpisodeAuditLogStore")

    // MARK: Configuration

    /// Hard cap on retained events per episode. When exceeded the oldest
    /// entries are dropped on the next append.
    let maxEventsPerEpisode: Int = 200

    // MARK: State

    /// Where the per-episode JSON files live. Mirrors `TranscriptStore`'s
    /// directory bootstrapping so the same Application Support container is
    /// shared by every persistence path.
    let rootURL: URL

    /// In-memory cache keyed by episode ID. Loaded lazily on first read so we
    /// don't walk the whole audit directory at launch. `@Observable` means SwiftUI
    /// re-renders the sheet whenever a new event lands for the displayed episode.
    private var cache: [UUID: [EpisodeAuditEvent]] = [:]

    // MARK: Init

    init(rootDirectory: URL? = nil) {
        if let rootDirectory {
            self.rootURL = rootDirectory
        } else {
            let support = (try? FileManager.default.url(
                for: .applicationSupportDirectory,
                in: .userDomainMask,
                appropriateFor: nil,
                create: true
            )) ?? FileManager.default.temporaryDirectory
            self.rootURL = support
                .appendingPathComponent("podcastr", isDirectory: true)
                .appendingPathComponent("audit", isDirectory: true)
        }
        try? FileManager.default.createDirectory(
            at: rootURL,
            withIntermediateDirectories: true
        )
    }

    // MARK: API

    /// Append `event` to the log for its episode. Idempotent on `event.id` —
    /// if a row with the same `id` already exists it is replaced.
    func append(_ event: EpisodeAuditEvent) {
        var list = events(for: event.episodeID)
        if let existing = list.firstIndex(where: { $0.id == event.id }) {
            list[existing] = event
        } else {
            list.append(event)
        }
        if list.count > maxEventsPerEpisode {
            list = Array(list.suffix(maxEventsPerEpisode))
        }
        cache[event.episodeID] = list
        persist(list, episodeID: event.episodeID)
    }

    /// Convenience builder. Captures the event and appends in one call.
    @discardableResult
    func record(
        episodeID: UUID,
        kind: EpisodeAuditEvent.Kind,
        severity: EpisodeAuditEvent.Severity = .info,
        summary: String,
        details: [EpisodeAuditEvent.Detail] = []
    ) -> EpisodeAuditEvent {
        let event = EpisodeAuditEvent(
            episodeID: episodeID,
            kind: kind,
            severity: severity,
            summary: summary,
            details: details
        )
        append(event)
        return event
    }

    /// Returns the events for `episodeID`, newest first.
    func events(for episodeID: UUID) -> [EpisodeAuditEvent] {
        if let cached = cache[episodeID] { return cached }
        let loaded = loadFromDisk(episodeID: episodeID)
        cache[episodeID] = loaded
        return loaded
    }

    /// Reverse-chronological view for the Diagnostics sheet.
    func eventsNewestFirst(for episodeID: UUID) -> [EpisodeAuditEvent] {
        events(for: episodeID).sorted { $0.timestamp > $1.timestamp }
    }

    /// Discards all events for `episodeID` (memory + disk).
    func clear(episodeID: UUID) {
        cache[episodeID] = []
        let url = fileURL(for: episodeID)
        try? FileManager.default.removeItem(at: url)
    }

    // MARK: - Persistence

    private func persist(_ events: [EpisodeAuditEvent], episodeID: UUID) {
        let url = fileURL(for: episodeID)
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.sortedKeys]
        do {
            let data = try encoder.encode(events)
            try data.write(to: url, options: .atomic)
        } catch {
            Self.logger.error(
                "persist failed for \(episodeID, privacy: .public): \(String(describing: error), privacy: .public)"
            )
        }
    }

    private func loadFromDisk(episodeID: UUID) -> [EpisodeAuditEvent] {
        let url = fileURL(for: episodeID)
        guard FileManager.default.fileExists(atPath: url.path) else { return [] }
        do {
            let data = try Data(contentsOf: url)
            let decoder = JSONDecoder()
            decoder.dateDecodingStrategy = .iso8601
            return try decoder.decode([EpisodeAuditEvent].self, from: data)
        } catch {
            Self.logger.error(
                "load failed for \(episodeID, privacy: .public): \(String(describing: error), privacy: .public)"
            )
            return []
        }
    }

    private func fileURL(for episodeID: UUID) -> URL {
        rootURL.appendingPathComponent("\(episodeID.uuidString).json")
    }
}
