import Foundation
import CSQLiteVec

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

/// SQLite sidecar for high-cardinality episode records.
///
/// `AppState` stays the in-memory model used by the UI, but persistence splits
/// episodes out of the JSON metadata blob so imported libraries do not require
/// a 70MB+ JSON decode/write on every launch or mutation.
struct EpisodeSQLiteStore: Sendable {
    let fileURL: URL

    func loadAll() throws -> [Episode] {
        try withDatabase { db in
            try ensureSchema(in: db)
            let statement = try prepare(
                """
                SELECT payload
                FROM episodes
                ORDER BY sort_order ASC
                """,
                in: db
            )
            defer { sqlite3_finalize(statement) }

            var episodes: [Episode] = []
            while true {
                let code = sqlite3_step(statement)
                if code == SQLITE_DONE { break }
                guard code == SQLITE_ROW else {
                    throw EpisodeSQLiteStoreError.step(Self.errorMessage(db))
                }
                guard let bytes = sqlite3_column_blob(statement, 0) else {
                    throw EpisodeSQLiteStoreError.decode("missing episode payload")
                }
                let count = Int(sqlite3_column_bytes(statement, 0))
                let data = Data(bytes: bytes, count: count)
                do {
                    episodes.append(try Self.decoder.decode(Episode.self, from: data))
                } catch {
                    throw EpisodeSQLiteStoreError.decode(error.localizedDescription)
                }
            }
            return episodes
        }
    }

    func replaceAll(_ episodes: [Episode]) throws {
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
                }
                try execute("COMMIT TRANSACTION", in: db)
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
        sortOrderUpdates: [EpisodeSQLiteSortOrderMutation]
    ) throws {
        guard !upserts.isEmpty || !deleteIDs.isEmpty || !sortOrderUpdates.isEmpty else {
            return
        }
        try withDatabase { db in
            try ensureSchema(in: db)
            try execute("BEGIN IMMEDIATE TRANSACTION", in: db)
            do {
                try deleteRows(deleteIDs, in: db)
                try upsertRows(upserts, in: db)
                try updateSortOrders(sortOrderUpdates, in: db)
                try execute("COMMIT TRANSACTION", in: db)
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

    private func withDatabase<T>(_ body: (OpaquePointer) throws -> T) throws -> T {
        try ensureParentDirectoryExists()
        var db: OpaquePointer?
        let flags = SQLITE_OPEN_CREATE | SQLITE_OPEN_READWRITE | SQLITE_OPEN_FULLMUTEX
        guard sqlite3_open_v2(fileURL.path, &db, flags, nil) == SQLITE_OK, let db else {
            let message = db.map(Self.errorMessage) ?? "sqlite3_open_v2 returned nil"
            if let db { sqlite3_close(db) }
            throw EpisodeSQLiteStoreError.open(message)
        }
        defer { sqlite3_close(db) }

        try execute("PRAGMA foreign_keys = ON", in: db)
        try execute("PRAGMA journal_mode = WAL", in: db)
        try execute("PRAGMA synchronous = NORMAL", in: db)
        return try body(db)
    }

    private func ensureSchema(in db: OpaquePointer) throws {
        try execute(
            """
            CREATE TABLE IF NOT EXISTS episodes(
                id TEXT PRIMARY KEY NOT NULL,
                subscription_id TEXT NOT NULL,
                guid TEXT NOT NULL,
                pub_date REAL NOT NULL,
                sort_order INTEGER NOT NULL,
                payload BLOB NOT NULL
            )
            """,
            in: db
        )
        try execute(
            """
            CREATE INDEX IF NOT EXISTS episodes_subscription_pubdate_idx
            ON episodes(subscription_id, pub_date DESC)
            """,
            in: db
        )
    }

    private func execute(_ sql: String, in db: OpaquePointer) throws {
        var error: UnsafeMutablePointer<CChar>?
        defer { sqlite3_free(error) }
        guard sqlite3_exec(db, sql, nil, nil, &error) == SQLITE_OK else {
            let message = error.map { String(cString: $0) } ?? Self.errorMessage(db)
            throw EpisodeSQLiteStoreError.execute(message)
        }
    }

    private func prepare(_ sql: String, in db: OpaquePointer) throws -> OpaquePointer {
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK,
              let statement else {
            throw EpisodeSQLiteStoreError.prepare(Self.errorMessage(db))
        }
        return statement
    }

    private func deleteRows(_ ids: [UUID], in db: OpaquePointer) throws {
        guard !ids.isEmpty else { return }
        let statement = try prepare("DELETE FROM episodes WHERE id = ?", in: db)
        defer { sqlite3_finalize(statement) }
        for id in ids {
            try bindText(id.uuidString, at: 1, to: statement, in: db)
            try step(statement, in: db)
            sqlite3_reset(statement)
            sqlite3_clear_bindings(statement)
        }
    }

    private func upsertRows(_ rows: [EpisodeSQLiteRowMutation], in db: OpaquePointer) throws {
        guard !rows.isEmpty else { return }
        let statement = try prepare(
            """
            INSERT INTO episodes(
                id, subscription_id, guid, pub_date, sort_order, payload
            ) VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                subscription_id = excluded.subscription_id,
                guid = excluded.guid,
                pub_date = excluded.pub_date,
                sort_order = excluded.sort_order,
                payload = excluded.payload
            """,
            in: db
        )
        defer { sqlite3_finalize(statement) }
        for row in rows {
            try bind(row.episode, sortOrder: row.sortOrder, to: statement, in: db)
            try step(statement, in: db)
            sqlite3_reset(statement)
            sqlite3_clear_bindings(statement)
        }
    }

    private func updateSortOrders(_ updates: [EpisodeSQLiteSortOrderMutation], in db: OpaquePointer) throws {
        guard !updates.isEmpty else { return }
        let statement = try prepare("UPDATE episodes SET sort_order = ? WHERE id = ?", in: db)
        defer { sqlite3_finalize(statement) }
        for update in updates {
            guard sqlite3_bind_int64(statement, 1, Int64(update.sortOrder)) == SQLITE_OK else {
                throw EpisodeSQLiteStoreError.bind(Self.errorMessage(db))
            }
            try bindText(update.id.uuidString, at: 2, to: statement, in: db)
            try step(statement, in: db)
            sqlite3_reset(statement)
            sqlite3_clear_bindings(statement)
        }
    }

    private func step(_ statement: OpaquePointer, in db: OpaquePointer) throws {
        let code = sqlite3_step(statement)
        guard code == SQLITE_DONE else {
            throw EpisodeSQLiteStoreError.step(Self.errorMessage(db))
        }
    }

    private func bind(_ episode: Episode, sortOrder: Int, to statement: OpaquePointer, in db: OpaquePointer) throws {
        let payload: Data
        do {
            payload = try Self.encoder.encode(episode)
        } catch {
            throw EpisodeSQLiteStoreError.bind(error.localizedDescription)
        }

        try bindText(episode.id.uuidString, at: 1, to: statement, in: db)
        // SQLite column is historically named `subscription_id`; it now
        // semantically holds `episode.podcastID`. Column rename intentionally
        // skipped to avoid migration risk for zero behavior win.
        try bindText(episode.podcastID.uuidString, at: 2, to: statement, in: db)
        try bindText(episode.guid, at: 3, to: statement, in: db)
        guard sqlite3_bind_double(statement, 4, episode.pubDate.timeIntervalSince1970) == SQLITE_OK,
              sqlite3_bind_int64(statement, 5, Int64(sortOrder)) == SQLITE_OK else {
            throw EpisodeSQLiteStoreError.bind(Self.errorMessage(db))
        }
        let code = payload.withUnsafeBytes { buffer in
            sqlite3_bind_blob(
                statement,
                6,
                buffer.baseAddress,
                Int32(payload.count),
                Self.transientDestructor
            )
        }
        guard code == SQLITE_OK else {
            throw EpisodeSQLiteStoreError.bind(Self.errorMessage(db))
        }
    }

    private func bindText(_ value: String, at index: Int32, to statement: OpaquePointer, in db: OpaquePointer) throws {
        let code = (value as NSString).utf8String.map {
            sqlite3_bind_text(statement, index, $0, -1, Self.transientDestructor)
        } ?? SQLITE_MISUSE
        guard code == SQLITE_OK else {
            throw EpisodeSQLiteStoreError.bind(Self.errorMessage(db))
        }
    }

    private func ensureParentDirectoryExists() throws {
        let parent = fileURL.deletingLastPathComponent()
        try FileManager.default.createDirectory(at: parent, withIntermediateDirectories: true)
    }

    private static var transientDestructor: sqlite3_destructor_type {
        unsafeBitCast(-1, to: sqlite3_destructor_type.self)
    }

    private static func errorMessage(_ db: OpaquePointer) -> String {
        String(cString: sqlite3_errmsg(db))
    }

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
}
