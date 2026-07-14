import Foundation
import os
import os.log

/// Persists `AppState` inside the shared App Group container.
///
/// Low-cardinality metadata stays in a JSON file. High-cardinality `Episode`
/// records live in a SQLite sidecar so a large imported library does not turn
/// every launch and mutation into a 70MB+ JSON decode/encode.
///
/// **Why a file, not `UserDefaults`.** A previous iteration wrote the blob to
/// `UserDefaults(suiteName: <App Group>)`. Once a user subscribed to a real
/// podcast, `state.episodes` ballooned the encoded blob past a few MB and
/// `cfprefsd` silently dropped the value on the next read — `data(forKey:)`
/// returned the previous (smaller) committed version, so anything written
/// after the size crossover was lost. The symptom was: complete onboarding,
/// kill the app, relaunch, and onboarding shows again because the
/// `hasCompletedOnboarding=true` save (which by then sat alongside the
/// episode list) never came back through the prefs daemon. `UserDefaults` is
/// fundamentally not designed for blobs of this size; the App Group
/// container's filesystem is.
///
/// **Atomic writes.** Saves go through `Data.write(to:options: .atomic)` so a
/// crash mid-write leaves the previous good blob in place rather than a
/// half-written file that would fail to decode and silently reset the user
/// to a fresh `AppState`.
///
/// **Production vs. tests.** Construct via `Persistence.shared` for production
/// (writes to `<app-group>/Library/Application Support/podcastr-state.v1.json`).
/// Tests construct an isolated instance against a unique temp file URL so
/// fixtures never leak into the real app's storage — the bug where launching
/// the app after running the test target showed phantom "Test Show" /
/// "Episode e1" was caused by both contexts writing to the same App Group key.
final class Persistence: Sendable {

    /// Shared, production-default instance writing to the App Group container.
    static let shared = Persistence(
        fileURL: Persistence.appGroupStateFileURL,
        writeMode: .background
    )

    enum WriteMode: Sendable {
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

    /// File this instance reads from / writes to.
    let fileURL: URL
    let episodeStore: EpisodeSQLiteStore
    private let writeMode: WriteMode
    private let backgroundWriter = PersistenceBackgroundWriter()
    private let episodeSnapshot = OSAllocatedUnfairLock<EpisodeSQLiteSnapshot?>(initialState: nil)
    private let lastEpisodeWriteSummaryLock = OSAllocatedUnfairLock<EpisodeWriteSummary>(initialState: .none)

    /// Lock-protected count of successful `save(_:)` invocations. Production
    /// code never reads this; the per-second-write regression tests use it
    /// to assert the position-debounce coalesces N rapid updates into ≤ 2
    /// disk writes. Atomic so tests can sample it without coordinating with
    /// the main actor.
    private let saveCounter = OSAllocatedUnfairLock<Int>(initialState: 0)

    init(
        fileURL: URL,
        writeMode: WriteMode = .immediate,
        episodeStoreURL: URL? = nil
    ) {
        self.fileURL = fileURL
        self.episodeStore = EpisodeSQLiteStore(
            fileURL: episodeStoreURL ?? Self.episodeStoreURL(for: fileURL)
        )
        self.writeMode = writeMode
    }

    /// Returns the number of times `save(_:)` has been called on this instance.
    /// Test-only — production code has no reason to inspect this.
    var saveInvocationCount: Int {
        saveCounter.withLock { $0 }
    }

    /// Test-only diagnostic for the most recent episode-sidecar write plan.
    /// Production writes never branch on this; regression tests use it to prove
    /// small episode mutations go through row-level deltas instead of a full
    /// `DELETE` + reinsert.
    var lastEpisodeWriteSummary: EpisodeWriteSummary {
        lastEpisodeWriteSummaryLock.withLock { $0 }
    }

    /// Resets the save counter back to 0. Tests call this after the
    /// `AppStateStore` initialiser has performed its eager save so subsequent
    /// assertions count only the writes the test itself triggers.
    func resetSaveInvocationCount() {
        saveCounter.withLock { $0 = 0 }
    }

    func resetEpisodeWriteSummary() {
        lastEpisodeWriteSummaryLock.withLock { $0 = .none }
    }

    // MARK: - State persistence

    /// Encodes `state` to JSON and writes it atomically to `fileURL`.
    ///
    /// Intentionally non-throwing: encode/write failures are logged via
    /// `os.Logger` and the existing persisted file is left untouched (the
    /// `.atomic` write would refuse to clobber on failure anyway) so a
    /// transient encoder bug can't drop the user's library.
    func save(_ state: AppState) {
        switch writeMode {
        case .immediate:
            write(state)
        case .background:
            let writer = backgroundWriter
            Task.detached(priority: .utility) { [state, writer] in
                await writer.enqueue(state, persistence: self)
            }
        }
    }

    func write(_ state: AppState) {
        let snapshot = EpisodeSQLiteStore.snapshot(for: state.episodes)
        let previousSnapshot = episodeSnapshot.withLock { $0 }
        if previousSnapshot?.signature != snapshot.signature {
            do {
                let summary = try writeEpisodes(
                    state.episodes,
                    snapshot: snapshot,
                    previousSnapshot: previousSnapshot
                )
                episodeSnapshot.withLock { $0 = snapshot }
                lastEpisodeWriteSummaryLock.withLock { $0 = summary }
            } catch {
                Self.logger.error("Persistence.save: episode SQLite write failed: \(error, privacy: .public)")
                return
            }
        } else {
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

        let data: Data
        do {
            data = try Self.encoder.encode(Self.metadataState(from: state))
        } catch {
            Self.logger.error("Persistence.save: encode failed: \(error, privacy: .public)")
            return
        }
        do {
            try ensureParentDirectoryExists()
            try data.write(to: fileURL, options: [.atomic])
            saveCounter.withLock { $0 += 1 }
            Self.logger.info("Persistence.save: bytes=\(data.count, privacy: .public)")
        } catch {
            Self.logger.error("Persistence.save: write failed at \(self.fileURL.path, privacy: .public): \(error, privacy: .public)")
        }
    }

    /// Loads and decodes `AppState` from `fileURL`.
    ///
    /// - Returns: The previously saved `AppState`, or a fresh `AppState()`
    ///   when no persisted file exists yet (including the one-shot path
    ///   where this is the very first launch under the file-based backend
    ///   and there's also no legacy `UserDefaults` blob to migrate).
    /// - Throws: Any `DecodingError` produced by `JSONDecoder` when the
    ///   stored data cannot be decoded. Callers fall back to a default state.
    func load() throws -> AppState {
        if FileManager.default.fileExists(atPath: fileURL.path) {
            let data = try Data(contentsOf: fileURL)
            var state = try Self.decoder.decode(AppState.self, from: data)
            hydrateEpisodesPreservingMetadata(into: &state)
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
            let metadata = try Self.encoder.encode(Self.metadataState(from: migrated))
            try? ensureParentDirectoryExists()
            try? metadata.write(to: fileURL, options: [.atomic])
            Self.appGroupDefaults.removeObject(forKey: Self.legacyStateKey)
            Self.logger.info("Persistence.load: migrated \(legacyData.count, privacy: .public) bytes from legacy UserDefaults key")
            return migrated
        }
        return AppState()
    }

    /// Wipes the persisted `AppState` file. Intended for the "Erase all
    /// data" code path and for test cleanup. Idempotent — missing files
    /// are not an error.
    func reset() {
        try? FileManager.default.removeItem(at: fileURL)
        episodeStore.reset()
        episodeSnapshot.withLock { $0 = nil }
        resetEpisodeWriteSummary()
    }

    // MARK: - Suite resolution

    /// The App Group suite name.
    ///
    /// Reads `AppGroupIdentifier` from the main bundle's `Info.plist` so
    /// the value comes from the Tuist `APP_GROUP_IDENTIFIER` build setting
    /// and stays in sync with the entitlements automatically.
    ///
    /// For extension targets (e.g. WidgetKit) whose `Bundle.main` is the
    /// extension bundle, add `AppGroupIdentifier` to their `Info.plist`
    /// with the same `$(APP_GROUP_IDENTIFIER)` substitution.
    static var appGroupIdentifier: String {
        Bundle.main.object(forInfoDictionaryKey: "AppGroupIdentifier") as? String
            ?? "group.com.podcastr.app"   // compile-time fallback
    }

    /// `UserDefaults` instance for the App Group suite. Retained only for
    /// the legacy-blob migration in `load()`; production reads/writes go
    /// through the file backend.
    static var appGroupDefaults: UserDefaults {
        UserDefaults(suiteName: appGroupIdentifier) ?? .standard
    }

    /// Absolute file URL for the production state blob inside the App Group
    /// container. Falls back to the user's caches directory when the App
    /// Group entitlement is missing (e.g. a stripped-down developer build) —
    /// parity with the old `appGroupDefaults ?? .standard` fallback.
    static var appGroupStateFileURL: URL {
        let manager = FileManager.default
        let base: URL
        if let groupContainer = manager.containerURL(forSecurityApplicationGroupIdentifier: appGroupIdentifier) {
            base = groupContainer.appendingPathComponent("Library/Application Support", isDirectory: true)
        } else {
            // Fallback: app-local caches. The widget can't reach this, but
            // neither can it reach UserDefaults.standard — same trade-off
            // as the previous fallback.
            let caches = (try? manager.url(for: .cachesDirectory, in: .userDomainMask, appropriateFor: nil, create: true))
                ?? URL(fileURLWithPath: NSTemporaryDirectory(), isDirectory: true)
            base = caches
        }
        return base.appendingPathComponent("podcastr-state.v1.json", isDirectory: false)
    }

    static func episodeStoreURL(for stateFileURL: URL) -> URL {
        let baseName = stateFileURL.deletingPathExtension().lastPathComponent
        return stateFileURL
            .deletingLastPathComponent()
            .appendingPathComponent("\(baseName).episodes.sqlite", isDirectory: false)
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
        previousSnapshot: EpisodeSQLiteSnapshot?
    ) throws -> EpisodeWriteSummary {
        guard let previousSnapshot,
              let delta = Self.delta(from: previousSnapshot, to: snapshot, episodes: episodes) else {
            try episodeStore.replaceAll(episodes)
            return EpisodeWriteSummary(
                kind: .replaceAll,
                upsertCount: episodes.count,
                deleteCount: 0,
                sortOrderUpdateCount: 0,
                totalEpisodeCount: episodes.count
            )
        }

        guard !delta.isEmpty else {
            return EpisodeWriteSummary(
                kind: .none,
                upsertCount: 0,
                deleteCount: 0,
                sortOrderUpdateCount: 0,
                totalEpisodeCount: episodes.count
            )
        }

        do {
            try episodeStore.applyDelta(
                upserts: delta.upserts,
                deleteIDs: delta.deleteIDs,
                sortOrderUpdates: delta.sortOrderUpdates
            )
            return EpisodeWriteSummary(
                kind: .delta,
                upsertCount: delta.upserts.count,
                deleteCount: delta.deleteIDs.count,
                sortOrderUpdateCount: delta.sortOrderUpdates.count,
                totalEpisodeCount: episodes.count
            )
        } catch {
            Self.logger.error("Persistence.save: episode SQLite delta failed, rebuilding sidecar: \(error, privacy: .public)")
            try episodeStore.replaceAll(episodes)
            return EpisodeWriteSummary(
                kind: .replaceAll,
                upsertCount: episodes.count,
                deleteCount: 0,
                sortOrderUpdateCount: 0,
                totalEpisodeCount: episodes.count
            )
        }
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
        let data = try Self.encoder.encode(Self.metadataState(from: state))
        try ensureParentDirectoryExists()
        try data.write(to: fileURL, options: [.atomic])
    }

    private static func metadataState(from state: AppState) -> AppState {
        var metadata = state
        metadata.episodes = []
        return metadata
    }
}
