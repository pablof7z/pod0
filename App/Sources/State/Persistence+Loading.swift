import Foundation
import os.log

extension Persistence {
    /// Loads authoritative SQLite or performs one of the bounded legacy
    /// migrations. Once Rust chapter authority exists, legacy chapter/ad keys
    /// are skipped by the decoder rather than reconstructed as native state.
    func load(loadLegacyChapterAdjuncts: Bool = true) throws -> AppState {
        let decoder = Self.decoder(
            loadLegacyChapterAdjuncts: loadLegacyChapterAdjuncts
        )
        if let metadata = try episodeStore.loadMetadata() {
            var state = try decoder.decode(AppState.self, from: metadata)
            state.episodes = try episodeStore.loadAll(
                loadLegacyChapterAdjuncts: loadLegacyChapterAdjuncts
            )
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
            var state = try decoder.decode(AppState.self, from: data)
            hydrateEpisodesPreservingMetadata(
                into: &state,
                loadLegacyChapterAdjuncts: loadLegacyChapterAdjuncts
            )
            let generation = max(state.persistenceGeneration, 1)
            state.persistenceGeneration = generation
            guard write(state, revision: generation) else {
                throw EpisodeSQLiteStoreError.execute("Unable to commit legacy state migration")
            }
            return state
        }
        let sqliteOnlyEpisodes = try episodeStore.loadAll(
            loadLegacyChapterAdjuncts: loadLegacyChapterAdjuncts
        )
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
        // One-shot migration from the pre-file UserDefaults backend. Isolated
        // stores never enter this production-only path.
        if fileURL == Self.appGroupStateFileURL,
           let legacyData = Self.appGroupDefaults.data(forKey: Self.legacyStateKey) {
            var migrated = try decoder.decode(AppState.self, from: legacyData)
            try hydrateEpisodes(
                into: &migrated,
                loadLegacyChapterAdjuncts: loadLegacyChapterAdjuncts
            )
            let generation = max(migrated.persistenceGeneration, 1)
            migrated.persistenceGeneration = generation
            guard write(migrated, revision: generation) else {
                throw EpisodeSQLiteStoreError.execute("Unable to commit UserDefaults migration")
            }
            Self.appGroupDefaults.removeObject(forKey: Self.legacyStateKey)
            Self.logger.info(
                "Persistence.load: migrated \(legacyData.count, privacy: .public) bytes from legacy UserDefaults key"
            )
            return migrated
        }
        return AppState()
    }

    private static func decoder(loadLegacyChapterAdjuncts: Bool) -> JSONDecoder {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        decoder.userInfo[.loadLegacyChapterAdjuncts] = loadLegacyChapterAdjuncts
        return decoder
    }

    private func hydrateEpisodes(
        into state: inout AppState,
        loadLegacyChapterAdjuncts: Bool
    ) throws {
        let jsonEpisodes = state.episodes
        let sqliteEpisodes = try episodeStore.loadAll(
            loadLegacyChapterAdjuncts: loadLegacyChapterAdjuncts
        )
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

    private func hydrateEpisodesPreservingMetadata(
        into state: inout AppState,
        loadLegacyChapterAdjuncts: Bool
    ) {
        do {
            try hydrateEpisodes(
                into: &state,
                loadLegacyChapterAdjuncts: loadLegacyChapterAdjuncts
            )
        } catch {
            Self.logger.error(
                "Persistence.load: episode SQLite hydration failed: \(error, privacy: .public); preserving JSON metadata"
            )
        }
    }
}
