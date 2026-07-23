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
    let revision = OSAllocatedUnfairLock<UInt64>(initialState: 0)
    let lastWrittenRevision = OSAllocatedUnfairLock<UInt64>(initialState: 0)
    let episodeSnapshot = OSAllocatedUnfairLock<EpisodeSQLiteSnapshot?>(initialState: nil)
    private let lastEpisodeWriteSummaryLock = OSAllocatedUnfairLock<EpisodeWriteSummary>(initialState: .none)
    let sharedArtifactAuthority = OSAllocatedUnfairLock<(notes: Bool, clips: Bool, scheduledAgents: Bool, memories: Bool)>(initialState: (false, false, false, false))

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
                publishWorkflowCommitIfNeeded(for: jobs)
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
        publishWorkflowCommitIfNeeded(for: jobs)
        Self.logger.info("Persistence.save: metadata bytes=\(metadata.count, privacy: .public)")
        return true
    }

    func reset() {
        try? FileManager.default.removeItem(at: fileURL)
        episodeStore.reset()
        removeSharedCoreArtifacts()
        episodeSnapshot.withLock { $0 = nil }
        revision.withLock { $0 = 0 }
        lastWrittenRevision.withLock { $0 = 0 }
        sharedArtifactAuthority.withLock { $0 = (false, false, false, false) }
        resetEpisodeWriteSummary()
    }

    // MARK: - Static helpers

    static let logger = Logger.app("Persistence")
    /// Prior-art `UserDefaults` key the file backend migrates from on first
    /// run. Kept as a string constant (not exposed) so the migration path
    /// stays self-documenting.
    static let legacyStateKey = "podcastr.state.v1"

    private static let encoder: JSONEncoder = {
        let e = JSONEncoder()
        e.dateEncodingStrategy = .iso8601
        e.outputFormatting = [.sortedKeys]
        return e
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

    func writeMetadataSnapshot(_ state: AppState) throws {
        let data = try Self.encoder.encode(metadataState(from: state))
        try ensureParentDirectoryExists()
        try data.write(to: fileURL, options: [.atomic])
    }

}
