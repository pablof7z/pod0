import Foundation
import os
import os.log

/// SQLite-authoritative app-state persistence. JSON is migration-only.
final class Persistence: Sendable {

    /// Shared, production-default instance writing to the App Group container.
    static let shared = Persistence(
        fileURL: Persistence.appGroupStateFileURL,
        writeMode: .background
    )

    enum WriteMode: Equatable, Sendable {
        case immediate
        case background
    }

    enum EpisodeWriteKind: Equatable, Sendable {
        case none
        case replaceAll
        case delta
    }

    struct EpisodeWriteSummary: Equatable, Sendable {
        let kind: EpisodeWriteKind
        let upsertCount: Int
        let deleteCount: Int
        let sortOrderUpdateCount: Int
        let totalEpisodeCount: Int

        static let none = EpisodeWriteSummary(
            kind: .none,
            upsertCount: 0,
            deleteCount: 0,
            sortOrderUpdateCount: 0,
            totalEpisodeCount: 0
        )
    }

    let fileURL: URL
    let episodeStore: EpisodeSQLiteStore
    private let writeMode: WriteMode
    private let beforeBackgroundEnqueue: @Sendable (UInt64) async -> Void
    private let backgroundWriter = PersistenceBackgroundWriter()
    let writeLock = NSLock()
    private let revision = OSAllocatedUnfairLock<UInt64>(initialState: 0)
    private let lastWrittenRevision = OSAllocatedUnfairLock<UInt64>(initialState: 0)
    private let episodeSnapshot = OSAllocatedUnfairLock<EpisodeSQLiteSnapshot?>(initialState: nil)
    private let lastEpisodeWriteSummaryLock = OSAllocatedUnfairLock<EpisodeWriteSummary>(initialState: .none)
    let sharedArtifactAuthority = OSAllocatedUnfairLock<(notes: Bool, clips: Bool)>(initialState: (false, false))

    /// Successful disk-write count used by persistence regression tests.
    private let saveCounter = OSAllocatedUnfairLock<Int>(initialState: 0)

    init(
        fileURL: URL,
        writeMode: WriteMode = .immediate,
        episodeStoreURL: URL? = nil,
        beforeBackgroundEnqueue: @escaping @Sendable (UInt64) async -> Void = { _ in },
        faultInjector: @escaping @Sendable (EpisodeSQLiteFaultPoint) throws -> Void = { _ in }
    ) {
        self.fileURL = fileURL
        self.episodeStore = EpisodeSQLiteStore(
            fileURL: episodeStoreURL ?? Self.episodeStoreURL(for: fileURL),
            faultInjector: faultInjector
        )
        self.writeMode = writeMode
        self.beforeBackgroundEnqueue = beforeBackgroundEnqueue
    }

    var saveInvocationCount: Int {
        saveCounter.withLock { $0 }
    }

    /// Test-only diagnostic for the most recent authoritative episode write plan.
    /// Production writes never branch on this; regression tests use it to prove
    /// small episode mutations go through row-level deltas instead of a full
    /// `DELETE` + reinsert.
    var lastEpisodeWriteSummary: EpisodeWriteSummary {
        lastEpisodeWriteSummaryLock.withLock { $0 }
    }

    func resetSaveInvocationCount() {
        saveCounter.withLock { $0 = 0 }
    }

    func resetEpisodeWriteSummary() {
        lastEpisodeWriteSummaryLock.withLock { $0 = .none }
    }

    // MARK: - State persistence
    /// Queues or performs an atomic snapshot write without regressing revisions.
    @discardableResult
    func save(
        _ state: AppState,
        revision requestedRevision: UInt64? = nil,
        ensuring jobs: [DesiredJob] = []
    ) -> UInt64 {
        let nextRevision = revision.withLock { current in
            if let requestedRevision {
                current = max(current, requestedRevision)
                return requestedRevision
            }
            current += 1
            return current
        }
        var snapshot = state
        snapshot.persistenceGeneration = nextRevision
        switch writeMode {
        case .immediate:
            write(snapshot, revision: nextRevision, ensuring: jobs)
        case .background:
            let writer = backgroundWriter
            let enqueueBarrier = beforeBackgroundEnqueue
            Task.detached(priority: .utility) { [snapshot, writer] in
                await enqueueBarrier(nextRevision)
                await writer.enqueue(
                    revision: nextRevision,
                    state: snapshot,
                    jobs: jobs,
                    persistence: self
                )
            }
        }
        return nextRevision
    }

    func waitUntilWritten(_ revision: UInt64) async -> Bool {
        switch writeMode {
        case .immediate:
            return lastWrittenRevision.withLock { $0 >= revision }
        case .background:
            return await backgroundWriter.waitUntilWritten(revision)
        }
    }

    /// Durability boundary for lifecycle suspension. Returns only after the
    /// exact snapshot (or a newer revision) commits to authoritative SQLite.
    @discardableResult
    func flush(_ state: AppState) async -> Bool {
        let flushRevision = save(state)
        guard writeMode == .background else {
            return lastWrittenRevision.withLock { $0 >= flushRevision }
        }
        return await backgroundWriter.waitUntilWritten(flushRevision)
    }

    @discardableResult
    func write(
        _ state: AppState,
        revision writeRevision: UInt64,
        ensuring jobs: [DesiredJob] = []
    ) -> Bool {
        writeLock.withLock {
            writeLocked(state, revision: writeRevision, ensuring: jobs)
        }
    }

    private func writeLocked(
        _ sourceState: AppState,
        revision writeRevision: UInt64,
        ensuring jobs: [DesiredJob]
    ) -> Bool {
        guard lastWrittenRevision.withLock({ writeRevision > $0 }) else {
            guard !jobs.isEmpty else { return true }
            do {
                _ = try JobStore(fileURL: episodeStore.fileURL).ensureJobs(jobs)
                saveCounter.withLock { $0 += 1 }
                NotificationCenter.default.post(
                    name: .persistenceDidCommitWorkflowJobs,
                    object: self
                )
                return true
            } catch {
                Self.logger.error("Persistence.save: stale snapshot job commit failed: \(error, privacy: .public)")
                return false
            }
        }
        var state = sourceState
        state.persistenceGeneration = writeRevision
        let metadata: Data
        do {
            metadata = try Self.encoder.encode(metadataState(from: state))
        } catch {
            Self.logger.error("Persistence.save: encode failed: \(error, privacy: .public)")
            return false
        }
        let snapshot = EpisodeSQLiteStore.snapshot(for: state.episodes)
        let previousSnapshot = episodeSnapshot.withLock { $0 }
        if previousSnapshot?.signature != snapshot.signature {
            do {
                let summary = try writeEpisodes(
                    state.episodes,
                    snapshot: snapshot,
                    previousSnapshot: previousSnapshot,
                    generation: writeRevision,
                    metadata: metadata,
                    ensuring: jobs
                )
                episodeSnapshot.withLock { $0 = snapshot }
                lastEpisodeWriteSummaryLock.withLock { $0 = summary }
            } catch {
                Self.logger.error("Persistence.save: episode SQLite write failed: \(error, privacy: .public)")
                return false
            }
        } else {
            do {
                try episodeStore.commitMetadata(
                    metadata, generation: writeRevision, ensuring: jobs
                )
            } catch {
                Self.logger.error("Persistence.save: metadata transaction failed: \(error, privacy: .public)")
                return false
            }
            lastEpisodeWriteSummaryLock.withLock {
                $0 = EpisodeWriteSummary(
                    kind: .none,
                    upsertCount: 0,
                    deleteCount: 0,
                    sortOrderUpdateCount: 0,
                    totalEpisodeCount: snapshot.signature.count
                )
            }
        }

        lastWrittenRevision.withLock { $0 = max($0, writeRevision) }
        saveCounter.withLock { $0 += 1 }
        NotificationCenter.default.post(name: .persistenceDidCommitWorkflowJobs, object: self)
        Self.logger.info("Persistence.save: metadata bytes=\(metadata.count, privacy: .public)")
        return true
    }

    /// Loads and decodes `AppState` from `fileURL`.
    ///
    /// - Returns: The previously saved `AppState`, or a fresh `AppState()`
    ///   when authoritative SQLite and the one-shot legacy JSON source are
    ///   both absent (the normal first-launch path).
    /// - Throws: Any `DecodingError` produced by `JSONDecoder` when the
    ///   stored data cannot be decoded. Callers fall back to a default state.
    func load() throws -> AppState {
        if let metadata = try episodeStore.loadMetadata() {
            var state = try Self.decoder.decode(AppState.self, from: metadata)
            state.episodes = try episodeStore.loadAll()
            let loadedEpisodes = state.episodes
            let generation = try episodeStore.loadGeneration()
            state.persistenceGeneration = generation
            episodeSnapshot.withLock { $0 = EpisodeSQLiteStore.snapshot(for: loadedEpisodes) }
            revision.withLock { $0 = max($0, generation) }
            lastWrittenRevision.withLock { $0 = max($0, generation) }
            return state
        }
        if FileManager.default.fileExists(atPath: fileURL.path) {
            let data = try Data(contentsOf: fileURL)
            var state = try Self.decoder.decode(AppState.self, from: data)
            hydrateEpisodesPreservingMetadata(into: &state)
            let generation = max(state.persistenceGeneration, 1)
            state.persistenceGeneration = generation
            guard write(state, revision: generation) else {
                throw EpisodeSQLiteStoreError.execute("Unable to commit legacy state migration")
            }
            return state
        }
        let sqliteOnlyEpisodes = try episodeStore.loadAll()
        if !sqliteOnlyEpisodes.isEmpty {
            var state = AppState()
            state.episodes = sqliteOnlyEpisodes
            let generation = max(try episodeStore.loadGeneration(), 1)
            state.persistenceGeneration = generation
            guard write(state, revision: generation) else {
                throw EpisodeSQLiteStoreError.execute("Unable to commit SQLite-only migration")
            }
            return state
        }
        // One-shot migration: an earlier build wrote `AppState` to App Group
        // `UserDefaults` under `legacyStateKey`. If a user is launching the
        // first build that uses the file backend, recover whatever the prefs
        // daemon was still serving (which is small enough to round-trip) so
        // their settings + small libraries survive the upgrade. After a
        // successful migration we wipe the legacy key so we never read it
        // again. Migration only runs for `Persistence.shared`; isolated
        // test instances point at temp files and have no legacy data.
        if fileURL == Self.appGroupStateFileURL,
           let legacyData = Self.appGroupDefaults.data(forKey: Self.legacyStateKey) {
            var migrated = try Self.decoder.decode(AppState.self, from: legacyData)
            try hydrateEpisodes(into: &migrated)
            let generation = max(migrated.persistenceGeneration, 1)
            migrated.persistenceGeneration = generation
            guard write(migrated, revision: generation) else {
                throw EpisodeSQLiteStoreError.execute("Unable to commit UserDefaults migration")
            }
            Self.appGroupDefaults.removeObject(forKey: Self.legacyStateKey)
            Self.logger.info("Persistence.load: migrated \(legacyData.count, privacy: .public) bytes from legacy UserDefaults key")
            return migrated
        }
        return AppState()
    }

    func reset() {
        try? FileManager.default.removeItem(at: fileURL)
        episodeStore.reset()
        removeSharedCoreArtifacts()
        episodeSnapshot.withLock { $0 = nil }
        revision.withLock { $0 = 0 }
        lastWrittenRevision.withLock { $0 = 0 }
        sharedArtifactAuthority.withLock { $0 = (false, false) }
        resetEpisodeWriteSummary()
    }

    // MARK: - Static helpers

    private static let logger = Logger.app("Persistence")
    /// Prior-art `UserDefaults` key the file backend migrates from on first
    /// run. Kept as a string constant (not exposed) so the migration path
    /// stays self-documenting.
    private static let legacyStateKey = "podcastr.state.v1"

    private static let encoder: JSONEncoder = {
        let e = JSONEncoder()
        e.dateEncodingStrategy = .iso8601
        e.outputFormatting = [.sortedKeys]
        return e
    }()

    private static let decoder: JSONDecoder = {
        let d = JSONDecoder()
        d.dateDecodingStrategy = .iso8601
        return d
    }()

    private static let maxIncrementalEpisodePayloadChanges = 128
    private static let maxIncrementalEpisodeDeletes = 128

    private struct EpisodeSQLiteDelta {
        let upserts: [EpisodeSQLiteRowMutation]
        let deleteIDs: [UUID]
        let sortOrderUpdates: [EpisodeSQLiteSortOrderMutation]

        var isEmpty: Bool {
            upserts.isEmpty && deleteIDs.isEmpty && sortOrderUpdates.isEmpty
        }
    }

    /// Creates the parent directory tree for `fileURL` if it doesn't already
    /// exist. App Group containers ship with `Library/` but not necessarily
    /// `Library/Application Support/`; `Data.write` would fail with ENOENT
    /// if we didn't precreate the path.
    private func ensureParentDirectoryExists() throws {
        let parent = fileURL.deletingLastPathComponent()
        try FileManager.default.createDirectory(at: parent, withIntermediateDirectories: true)
    }

    private func writeEpisodes(
        _ episodes: [Episode],
        snapshot: EpisodeSQLiteSnapshot,
        previousSnapshot: EpisodeSQLiteSnapshot?,
        generation: UInt64,
        metadata: Data,
        ensuring jobs: [DesiredJob]
    ) throws -> EpisodeWriteSummary {
        guard let previousSnapshot,
              let delta = Self.delta(from: previousSnapshot, to: snapshot, episodes: episodes) else {
            try episodeStore.replaceAll(
                episodes, generation: generation, metadata: metadata, ensuring: jobs
            )
            return EpisodeWriteSummary(
                kind: .replaceAll,
                upsertCount: episodes.count,
                deleteCount: 0,
                sortOrderUpdateCount: 0,
                totalEpisodeCount: episodes.count
            )
        }

        guard !delta.isEmpty else {
            try episodeStore.commitMetadata(metadata, generation: generation, ensuring: jobs)
            return EpisodeWriteSummary(
                kind: .none,
                upsertCount: 0,
                deleteCount: 0,
                sortOrderUpdateCount: 0,
                totalEpisodeCount: episodes.count
            )
        }

        try episodeStore.applyDelta(
            upserts: delta.upserts,
            deleteIDs: delta.deleteIDs,
            sortOrderUpdates: delta.sortOrderUpdates,
            generation: generation,
            metadata: metadata,
            ensuring: jobs
        )
        return EpisodeWriteSummary(
            kind: .delta,
            upsertCount: delta.upserts.count,
            deleteCount: delta.deleteIDs.count,
            sortOrderUpdateCount: delta.sortOrderUpdates.count,
            totalEpisodeCount: episodes.count
        )
    }

    private static func delta(
        from previous: EpisodeSQLiteSnapshot,
        to current: EpisodeSQLiteSnapshot,
        episodes: [Episode]
    ) -> EpisodeSQLiteDelta? {
        guard previous.hasUniqueIDs, current.hasUniqueIDs else { return nil }

        let deleteIDs = previous.rows.compactMap { row in
            current.indexByID[row.id] == nil ? row.id : nil
        }
        guard deleteIDs.count <= maxIncrementalEpisodeDeletes else { return nil }

        var upserts: [EpisodeSQLiteRowMutation] = []
        var sortOrderUpdates: [EpisodeSQLiteSortOrderMutation] = []
        upserts.reserveCapacity(min(episodes.count, maxIncrementalEpisodePayloadChanges))

        for (index, row) in current.rows.enumerated() {
            guard let previousIndex = previous.indexByID[row.id] else {
                upserts.append(EpisodeSQLiteRowMutation(episode: episodes[index], sortOrder: index))
                continue
            }

            let previousRow = previous.rows[previousIndex]
            if previousRow.payloadHash != row.payloadHash {
                upserts.append(EpisodeSQLiteRowMutation(episode: episodes[index], sortOrder: index))
            } else if previousIndex != index {
                sortOrderUpdates.append(EpisodeSQLiteSortOrderMutation(id: row.id, sortOrder: index))
            }
        }

        guard upserts.count <= maxIncrementalEpisodePayloadChanges else { return nil }
        return EpisodeSQLiteDelta(
            upserts: upserts,
            deleteIDs: deleteIDs,
            sortOrderUpdates: sortOrderUpdates
        )
    }

    private func hydrateEpisodes(into state: inout AppState) throws {
        let jsonEpisodes = state.episodes
        let sqliteEpisodes = try episodeStore.loadAll()
        if sqliteEpisodes.isEmpty {
            guard !jsonEpisodes.isEmpty else {
                episodeSnapshot.withLock { $0 = EpisodeSQLiteStore.snapshot(for: []) }
                return
            }
            try episodeStore.replaceAll(jsonEpisodes)
            episodeSnapshot.withLock {
                $0 = EpisodeSQLiteStore.snapshot(for: jsonEpisodes)
            }
            try writeMetadataSnapshot(state)
            return
        }

        state.episodes = sqliteEpisodes
        episodeSnapshot.withLock {
            $0 = EpisodeSQLiteStore.snapshot(for: sqliteEpisodes)
        }
        if !jsonEpisodes.isEmpty {
            try writeMetadataSnapshot(state)
        }
    }

    private func hydrateEpisodesPreservingMetadata(into state: inout AppState) {
        do {
            try hydrateEpisodes(into: &state)
        } catch {
            Self.logger.error("Persistence.load: episode SQLite hydration failed: \(error, privacy: .public); preserving JSON metadata")
        }
    }

    private func writeMetadataSnapshot(_ state: AppState) throws {
        let data = try Self.encoder.encode(metadataState(from: state))
        try ensureParentDirectoryExists()
        try data.write(to: fileURL, options: [.atomic])
    }

}
