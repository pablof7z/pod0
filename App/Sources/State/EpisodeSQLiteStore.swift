import Foundation
import CSQLite3

struct EpisodeSQLiteSignature: Equatable, Sendable {
    let count: Int
    let hash: Int
}

struct EpisodeSQLiteRowSnapshot: Equatable, Sendable {
    let id: UUID
    let payloadHash: Int
}

struct EpisodeSQLiteSnapshot: Equatable, Sendable {
    let signature: EpisodeSQLiteSignature
    let rows: [EpisodeSQLiteRowSnapshot]
    let indexByID: [UUID: Int]

    var hasUniqueIDs: Bool {
        indexByID.count == rows.count
    }
}

struct EpisodeSQLiteRowMutation: Sendable {
    let episode: Episode
    let sortOrder: Int
}

struct EpisodeSQLiteSortOrderMutation: Sendable {
    let id: UUID
    let sortOrder: Int
}

enum EpisodeSQLiteStoreError: LocalizedError {
    case open(String)
    case execute(String)
    case prepare(String)
    case bind(String)
    case step(String)
    case decode(String)

    var errorDescription: String? {
        switch self {
        case .open(let message):
            return "Episode store open failed: \(message)"
        case .execute(let message):
            return "Episode store statement failed: \(message)"
        case .prepare(let message):
            return "Episode store prepare failed: \(message)"
        case .bind(let message):
            return "Episode store bind failed: \(message)"
        case .step(let message):
            return "Episode store step failed: \(message)"
        case .decode(let message):
            return "Episode store decode failed: \(message)"
        }
    }
}

enum EpisodeSQLiteFaultPoint: Equatable, Sendable {
    case afterEpisodeStatement(Int)
    case afterMetadataStatement
    case afterJobStatement(Int)
    case beforeCommit
    case afterCommit
}

/// Authoritative SQLite store for app metadata, episodes, and workflow jobs.
///
/// `AppState` stays the in-memory model used by the UI, but persistence splits
/// episodes out of the JSON metadata blob so imported libraries do not require
/// a 70MB+ JSON decode/write on every launch or mutation.
struct EpisodeSQLiteStore: Sendable {
    let fileURL: URL
    let faultInjector: @Sendable (EpisodeSQLiteFaultPoint) throws -> Void

    init(
        fileURL: URL,
        faultInjector: @escaping @Sendable (EpisodeSQLiteFaultPoint) throws -> Void = { _ in }
    ) {
        self.fileURL = fileURL
        self.faultInjector = faultInjector
    }

    func replaceAll(
        _ episodes: [Episode],
        generation: UInt64? = nil,
        metadata: Data? = nil,
        ensuring jobs: [DesiredJob] = []
    ) throws {
        try withDatabase { db in
            try ensureSchema(in: db)
            try execute("BEGIN IMMEDIATE TRANSACTION", in: db)
            do {
                try execute("DELETE FROM episodes", in: db)
                let statement = try prepare(
                    """
                    INSERT INTO episodes(
                        id, subscription_id, guid, pub_date, sort_order, payload
                    ) VALUES (?, ?, ?, ?, ?, ?)
                    """,
                    in: db
                )
                defer { sqlite3_finalize(statement) }

                for (index, episode) in episodes.enumerated() {
                    try bind(episode, sortOrder: index, to: statement, in: db)
                    let code = sqlite3_step(statement)
                    guard code == SQLITE_DONE else {
                        throw EpisodeSQLiteStoreError.step(Self.errorMessage(db))
                    }
                    sqlite3_reset(statement)
                    sqlite3_clear_bindings(statement)
                    try faultInjector(.afterEpisodeStatement(index))
                }
                if let generation { try writeGeneration(generation, in: db) }
                if let metadata {
                    try writeMetadata(metadata, in: db)
                    try faultInjector(.afterMetadataStatement)
                }
                try JobStore.ensureJobs(jobs, in: db) { index in
                    try faultInjector(.afterJobStatement(index))
                }
                try faultInjector(.beforeCommit)
                try execute("COMMIT TRANSACTION", in: db)
                try faultInjector(.afterCommit)
            } catch {
                try? execute("ROLLBACK TRANSACTION", in: db)
                throw error
            }
        }
    }

    func upsert(_ rows: [EpisodeSQLiteRowMutation]) throws {
        try applyDelta(upserts: rows, deleteIDs: [], sortOrderUpdates: [])
    }

    func delete(ids: [UUID]) throws {
        try applyDelta(upserts: [], deleteIDs: ids, sortOrderUpdates: [])
    }

    func applyDelta(
        upserts: [EpisodeSQLiteRowMutation],
        deleteIDs: [UUID],
        sortOrderUpdates: [EpisodeSQLiteSortOrderMutation],
        generation: UInt64? = nil,
        metadata: Data? = nil,
        ensuring jobs: [DesiredJob] = []
    ) throws {
        guard !upserts.isEmpty || !deleteIDs.isEmpty || !sortOrderUpdates.isEmpty
                || generation != nil || metadata != nil || !jobs.isEmpty else {
            return
        }
        try withDatabase { db in
            try ensureSchema(in: db)
            try execute("BEGIN IMMEDIATE TRANSACTION", in: db)
            do {
                var statementIndex = 0
                let didMutateEpisode: () throws -> Void = {
                    try faultInjector(.afterEpisodeStatement(statementIndex))
                    statementIndex += 1
                }
                try deleteRows(deleteIDs, in: db, afterEach: didMutateEpisode)
                try upsertRows(upserts, in: db, afterEach: didMutateEpisode)
                try updateSortOrders(sortOrderUpdates, in: db, afterEach: didMutateEpisode)
                if let generation { try writeGeneration(generation, in: db) }
                if let metadata {
                    try writeMetadata(metadata, in: db)
                    try faultInjector(.afterMetadataStatement)
                }
                try JobStore.ensureJobs(jobs, in: db) { index in
                    try faultInjector(.afterJobStatement(index))
                }
                try faultInjector(.beforeCommit)
                try execute("COMMIT TRANSACTION", in: db)
                try faultInjector(.afterCommit)
            } catch {
                try? execute("ROLLBACK TRANSACTION", in: db)
                throw error
            }
        }
    }

    func reset() {
        for suffix in ["", "-wal", "-shm"] {
            try? FileManager.default.removeItem(
                at: URL(fileURLWithPath: fileURL.path + suffix)
            )
        }
    }

    func loadGeneration() throws -> UInt64 {
        try withDatabase { db in
            try ensureSchema(in: db)
            let statement = try prepare(
                "SELECT value FROM persistence_metadata WHERE key = 'generation'",
                in: db
            )
            defer { sqlite3_finalize(statement) }
            guard sqlite3_step(statement) == SQLITE_ROW,
                  let text = sqlite3_column_text(statement, 0) else { return 0 }
            return UInt64(String(cString: text)) ?? 0
        }
    }

    func setGeneration(_ generation: UInt64) throws {
        try withDatabase { db in
            try ensureSchema(in: db)
            try writeGeneration(generation, in: db)
        }
    }

    func loadMetadata() throws -> Data? {
        try withDatabase { db in
            try ensureSchema(in: db)
            let statement = try prepare(
                "SELECT value FROM persistence_metadata WHERE key = 'app_state'",
                in: db
            )
            defer { sqlite3_finalize(statement) }
            guard sqlite3_step(statement) == SQLITE_ROW,
                  let bytes = sqlite3_column_blob(statement, 0) else { return nil }
            return Data(bytes: bytes, count: Int(sqlite3_column_bytes(statement, 0)))
        }
    }

    func commitMetadata(
        _ metadata: Data,
        generation: UInt64,
        ensuring jobs: [DesiredJob] = []
    ) throws {
        try applyDelta(
            upserts: [], deleteIDs: [], sortOrderUpdates: [],
            generation: generation, metadata: metadata, ensuring: jobs
        )
    }

    static func signature(for episodes: [Episode]) -> EpisodeSQLiteSignature {
        snapshot(for: episodes).signature
    }

    static func snapshot(for episodes: [Episode]) -> EpisodeSQLiteSnapshot {
        var hasher = Hasher()
        var rows: [EpisodeSQLiteRowSnapshot] = []
        var indexByID: [UUID: Int] = [:]
        rows.reserveCapacity(episodes.count)
        indexByID.reserveCapacity(episodes.count)
        for (index, episode) in episodes.enumerated() {
            var rowHasher = Hasher()
            rowHasher.combine(episode)
            let payloadHash = rowHasher.finalize()
            rows.append(EpisodeSQLiteRowSnapshot(id: episode.id, payloadHash: payloadHash))
            indexByID[episode.id] = index
            hasher.combine(episode.id)
            hasher.combine(payloadHash)
        }
        return EpisodeSQLiteSnapshot(
            signature: EpisodeSQLiteSignature(count: episodes.count, hash: hasher.finalize()),
            rows: rows,
            indexByID: indexByID
        )
    }

}
