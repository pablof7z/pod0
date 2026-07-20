import CSQLiteVec
import Foundation

extension EpisodeSQLiteStore {
    func withDatabase<T>(_ body: (OpaquePointer) throws -> T) throws -> T {
        WorkflowSQLite.databaseLock.lock()
        defer { WorkflowSQLite.databaseLock.unlock() }
        try ensureParentDirectoryExists()
        var db: OpaquePointer?
        let flags = SQLITE_OPEN_CREATE | SQLITE_OPEN_READWRITE | SQLITE_OPEN_FULLMUTEX
        guard sqlite3_open_v2(fileURL.path, &db, flags, nil) == SQLITE_OK, let db else {
            let message = db.map(Self.errorMessage) ?? "sqlite3_open_v2 returned nil"
            if let db { sqlite3_close(db) }
            throw EpisodeSQLiteStoreError.open(message)
        }
        defer { sqlite3_close(db) }
        sqlite3_busy_timeout(db, 5_000)
        try execute("PRAGMA foreign_keys = ON", in: db)
        try execute("PRAGMA journal_mode = WAL", in: db)
        try execute("PRAGMA synchronous = NORMAL", in: db)
        return try body(db)
    }

    func ensureSchema(in db: OpaquePointer) throws {
        try execute("""
            CREATE TABLE IF NOT EXISTS episodes(
                id TEXT PRIMARY KEY NOT NULL, subscription_id TEXT NOT NULL,
                guid TEXT NOT NULL, pub_date REAL NOT NULL,
                sort_order INTEGER NOT NULL, payload BLOB NOT NULL
            )
            """, in: db)
        try execute("""
            CREATE INDEX IF NOT EXISTS episodes_subscription_pubdate_idx
            ON episodes(subscription_id, pub_date DESC)
            """, in: db)
        try execute("""
            CREATE TABLE IF NOT EXISTS persistence_metadata(
                key TEXT PRIMARY KEY NOT NULL, value BLOB NOT NULL
            )
            """, in: db)
    }

    func writeGeneration(_ generation: UInt64, in db: OpaquePointer) throws {
        let statement = try prepare("""
            INSERT INTO persistence_metadata(key, value) VALUES ('generation', ?)
            ON CONFLICT(key) DO UPDATE SET value = excluded.value
            """, in: db)
        defer { sqlite3_finalize(statement) }
        try bindText(String(generation), at: 1, to: statement, in: db)
        try step(statement, in: db)
    }

    func writeMetadata(_ metadata: Data, in db: OpaquePointer) throws {
        let statement = try prepare("""
            INSERT INTO persistence_metadata(key, value) VALUES ('app_state', ?)
            ON CONFLICT(key) DO UPDATE SET value = excluded.value
            """, in: db)
        defer { sqlite3_finalize(statement) }
        let code = metadata.withUnsafeBytes { buffer in
            sqlite3_bind_blob(statement, 1, buffer.baseAddress, Int32(buffer.count), Self.transientDestructor)
        }
        guard code == SQLITE_OK else { throw EpisodeSQLiteStoreError.bind(Self.errorMessage(db)) }
        try step(statement, in: db)
    }

    func execute(_ sql: String, in db: OpaquePointer) throws {
        var error: UnsafeMutablePointer<CChar>?
        defer { sqlite3_free(error) }
        guard sqlite3_exec(db, sql, nil, nil, &error) == SQLITE_OK else {
            throw EpisodeSQLiteStoreError.execute(
                error.map { String(cString: $0) } ?? Self.errorMessage(db)
            )
        }
    }

    func prepare(_ sql: String, in db: OpaquePointer) throws -> OpaquePointer {
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK,
              let statement else {
            throw EpisodeSQLiteStoreError.prepare(Self.errorMessage(db))
        }
        return statement
    }

    func deleteRows(
        _ ids: [UUID], in db: OpaquePointer, afterEach: () throws -> Void = {}
    ) throws {
        guard !ids.isEmpty else { return }
        let statement = try prepare("DELETE FROM episodes WHERE id = ?", in: db)
        defer { sqlite3_finalize(statement) }
        for id in ids {
            try bindText(id.uuidString, at: 1, to: statement, in: db)
            try step(statement, in: db); try afterEach()
            sqlite3_reset(statement); sqlite3_clear_bindings(statement)
        }
    }

    func upsertRows(
        _ rows: [EpisodeSQLiteRowMutation], in db: OpaquePointer,
        afterEach: () throws -> Void = {}
    ) throws {
        guard !rows.isEmpty else { return }
        let statement = try prepare("""
            INSERT INTO episodes(id,subscription_id,guid,pub_date,sort_order,payload)
            VALUES(?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET
                subscription_id=excluded.subscription_id, guid=excluded.guid,
                pub_date=excluded.pub_date, sort_order=excluded.sort_order,
                payload=excluded.payload
            """, in: db)
        defer { sqlite3_finalize(statement) }
        for row in rows {
            try bind(row.episode, sortOrder: row.sortOrder, to: statement, in: db)
            try step(statement, in: db); try afterEach()
            sqlite3_reset(statement); sqlite3_clear_bindings(statement)
        }
    }

    func updateSortOrders(
        _ updates: [EpisodeSQLiteSortOrderMutation], in db: OpaquePointer,
        afterEach: () throws -> Void = {}
    ) throws {
        guard !updates.isEmpty else { return }
        let statement = try prepare("UPDATE episodes SET sort_order=? WHERE id=?", in: db)
        defer { sqlite3_finalize(statement) }
        for update in updates {
            guard sqlite3_bind_int64(statement, 1, Int64(update.sortOrder)) == SQLITE_OK else {
                throw EpisodeSQLiteStoreError.bind(Self.errorMessage(db))
            }
            try bindText(update.id.uuidString, at: 2, to: statement, in: db)
            try step(statement, in: db); try afterEach()
            sqlite3_reset(statement); sqlite3_clear_bindings(statement)
        }
    }

    func step(_ statement: OpaquePointer, in db: OpaquePointer) throws {
        guard sqlite3_step(statement) == SQLITE_DONE else {
            throw EpisodeSQLiteStoreError.step(Self.errorMessage(db))
        }
    }

    func bind(
        _ episode: Episode, sortOrder: Int,
        to statement: OpaquePointer, in db: OpaquePointer
    ) throws {
        let payload: Data
        do { payload = try Self.encoder.encode(episode) }
        catch { throw EpisodeSQLiteStoreError.bind(error.localizedDescription) }
        try bindText(episode.id.uuidString, at: 1, to: statement, in: db)
        try bindText(episode.podcastID.uuidString, at: 2, to: statement, in: db)
        try bindText(episode.guid, at: 3, to: statement, in: db)
        guard sqlite3_bind_double(statement, 4, episode.pubDate.timeIntervalSince1970) == SQLITE_OK,
              sqlite3_bind_int64(statement, 5, Int64(sortOrder)) == SQLITE_OK else {
            throw EpisodeSQLiteStoreError.bind(Self.errorMessage(db))
        }
        let code = payload.withUnsafeBytes {
            sqlite3_bind_blob(statement, 6, $0.baseAddress, Int32($0.count), Self.transientDestructor)
        }
        guard code == SQLITE_OK else { throw EpisodeSQLiteStoreError.bind(Self.errorMessage(db)) }
    }

    func bindText(
        _ value: String, at index: Int32,
        to statement: OpaquePointer, in db: OpaquePointer
    ) throws {
        let code = (value as NSString).utf8String.map {
            sqlite3_bind_text(statement, index, $0, -1, Self.transientDestructor)
        } ?? SQLITE_MISUSE
        guard code == SQLITE_OK else { throw EpisodeSQLiteStoreError.bind(Self.errorMessage(db)) }
    }

    func ensureParentDirectoryExists() throws {
        try FileManager.default.createDirectory(
            at: fileURL.deletingLastPathComponent(), withIntermediateDirectories: true
        )
    }

    static var transientDestructor: sqlite3_destructor_type {
        unsafeBitCast(-1, to: sqlite3_destructor_type.self)
    }

    static func errorMessage(_ db: OpaquePointer) -> String {
        String(cString: sqlite3_errmsg(db))
    }

    static let encoder: JSONEncoder = {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.sortedKeys]
        return encoder
    }()

    static func decoder(loadLegacyChapterAdjuncts: Bool) -> JSONDecoder {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        decoder.userInfo[.loadLegacyChapterAdjuncts] = loadLegacyChapterAdjuncts
        return decoder
    }
}
