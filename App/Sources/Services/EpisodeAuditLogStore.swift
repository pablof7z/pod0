import Foundation
import os.log

// MARK: - EpisodeAuditLogStore

/// Append-only per-episode audit log persisted as JSON.
///
/// Files live under `$applicationSupport/podcastr/audit/<episodeID>.json` —
/// the same per-episode `Application Support` shape as other artifact stores and
/// the app's durable stores so a single fallback to `temporaryDirectory` covers
/// every persistence path when the container is unavailable.
///
/// Concurrency: observable state stays on `@MainActor`; ordered disk work runs
/// through a background tail so opening Diagnostics and workflow event bursts
/// never perform file I/O on the UI executor.
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

    /// Where the per-episode JSON files live. Uses the standard artifact-store
    /// directory bootstrapping so the same Application Support container is
    /// shared by every persistence path.
    let rootURL: URL
    private let diskStore: EpisodeAuditLogDiskStore

    /// In-memory cache keyed by episode ID. Loaded lazily on first read so we
    /// don't walk the whole audit directory at launch. `@Observable` means SwiftUI
    /// re-renders the sheet whenever a new event lands for the displayed episode.
    private var cache: [UUID: [EpisodeAuditEvent]] = [:]
    private var loadRequested: Set<UUID> = []
    private var cacheRevisions: [UUID: UInt64] = [:]
    private var diskTail: Task<Void, Never>?

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
        diskStore = EpisodeAuditLogDiskStore(rootURL: rootURL)
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
        let revision = advanceCacheRevision(for: event.episodeID)
        enqueueDiskWork { [weak self] diskStore in
            let persisted = await diskStore.append(
                event,
                maximumCount: self?.maxEventsPerEpisode ?? 200
            )
            guard let self, cacheRevisions[event.episodeID] == revision else { return }
            let current = cache[event.episodeID] ?? []
            let byID = Dictionary(
                (persisted + current).map { ($0.id, $0) },
                uniquingKeysWith: { _, latest in latest }
            )
            cache[event.episodeID] = Array(
                byID.values
                    .sorted { $0.timestamp < $1.timestamp }
                    .suffix(maxEventsPerEpisode)
            )
        }
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
        cache[episodeID] = []
        guard loadRequested.insert(episodeID).inserted else { return [] }
        let revision = cacheRevisions[episodeID, default: 0]
        enqueueDiskWork { [weak self] diskStore in
            let loaded = await diskStore.load(episodeID: episodeID)
            guard let self, cacheRevisions[episodeID, default: 0] == revision else { return }
            let current = cache[episodeID] ?? []
            let byID = Dictionary(
                (loaded + current).map { ($0.id, $0) },
                uniquingKeysWith: { _, latest in latest }
            )
            let merged = byID.values
                .sorted { $0.timestamp < $1.timestamp }
                .suffix(maxEventsPerEpisode)
            cache[episodeID] = Array(merged)
        }
        return []
    }

    /// Reverse-chronological view for the Diagnostics sheet.
    func eventsNewestFirst(for episodeID: UUID) -> [EpisodeAuditEvent] {
        events(for: episodeID).sorted { $0.timestamp > $1.timestamp }
    }

    /// Discards all events for `episodeID` (memory + disk).
    func clear(episodeID: UUID) {
        cache[episodeID] = []
        _ = advanceCacheRevision(for: episodeID)
        enqueueDiskWork { diskStore in
            await diskStore.clear(episodeID: episodeID)
        }
    }

    // MARK: - Persistence

    private func enqueueDiskWork(
        _ operation: @escaping @MainActor (EpisodeAuditLogDiskStore) async -> Void
    ) {
        let previous = diskTail
        let diskStore = diskStore
        diskTail = Task { @MainActor in
            await previous?.value
            await operation(diskStore)
        }
    }

    private func advanceCacheRevision(for episodeID: UUID) -> UInt64 {
        let revision = cacheRevisions[episodeID, default: 0]
        let next = revision == .max ? .max : revision + 1
        cacheRevisions[episodeID] = next
        return next
    }
}

private actor EpisodeAuditLogDiskStore {
    let rootURL: URL
    private var cache: [UUID: [EpisodeAuditEvent]] = [:]

    init(rootURL: URL) {
        self.rootURL = rootURL
    }

    func append(
        _ event: EpisodeAuditEvent,
        maximumCount: Int
    ) -> [EpisodeAuditEvent] {
        var events = load(episodeID: event.episodeID)
        if let index = events.firstIndex(where: { $0.id == event.id }) {
            events[index] = event
        } else {
            events.append(event)
        }
        events = Array(
            events.sorted { $0.timestamp < $1.timestamp }.suffix(maximumCount)
        )
        persist(events, episodeID: event.episodeID)
        return events
    }

    private func persist(_ events: [EpisodeAuditEvent], episodeID: UUID) {
        do {
            try FileManager.default.createDirectory(
                at: rootURL,
                withIntermediateDirectories: true
            )
            let encoder = JSONEncoder()
            encoder.dateEncodingStrategy = .iso8601
            encoder.outputFormatting = [.sortedKeys]
            try encoder.encode(events).write(to: fileURL(for: episodeID), options: .atomic)
            cache[episodeID] = events
        } catch {
            Logger.app("EpisodeAuditLogStore").error(
                "persist failed for \(episodeID, privacy: .public)"
            )
        }
    }

    func load(episodeID: UUID) -> [EpisodeAuditEvent] {
        if let cached = cache[episodeID] { return cached }
        let url = fileURL(for: episodeID)
        guard FileManager.default.fileExists(atPath: url.path) else { return [] }
        do {
            let decoder = JSONDecoder()
            decoder.dateDecodingStrategy = .iso8601
            let loaded = try decoder.decode([EpisodeAuditEvent].self, from: Data(contentsOf: url))
            cache[episodeID] = loaded
            return loaded
        } catch {
            Logger.app("EpisodeAuditLogStore").error(
                "load failed for \(episodeID, privacy: .public)"
            )
            return []
        }
    }

    func clear(episodeID: UUID) {
        cache[episodeID] = []
        try? FileManager.default.removeItem(at: fileURL(for: episodeID))
    }

    private func fileURL(for episodeID: UUID) -> URL {
        rootURL.appendingPathComponent("\(episodeID.uuidString).json")
    }
}
