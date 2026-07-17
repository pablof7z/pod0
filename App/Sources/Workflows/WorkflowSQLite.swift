import CSQLiteVec
import Foundation

enum WorkflowSQLite {
    static let databaseLock = NSRecursiveLock()

    static func withDatabase<T>(fileURL: URL, _ body: (OpaquePointer) throws -> T) throws -> T {
        databaseLock.lock()
        defer { databaseLock.unlock() }
        try FileManager.default.createDirectory(
            at: fileURL.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        var database: OpaquePointer?
        let flags = SQLITE_OPEN_CREATE | SQLITE_OPEN_READWRITE | SQLITE_OPEN_FULLMUTEX
        guard sqlite3_open_v2(fileURL.path, &database, flags, nil) == SQLITE_OK,
              let database else {
            if let database { sqlite3_close(database) }
            throw JobStoreError.sqlite("Unable to open workflow database")
        }
        defer { sqlite3_close(database) }
        sqlite3_busy_timeout(database, 5_000)
        try execute("PRAGMA synchronous=NORMAL", database)
        return try body(database)
    }

    static func execute(_ sql: String, _ db: OpaquePointer) throws {
        guard sqlite3_exec(db, sql, nil, nil, nil) == SQLITE_OK else { throw error(db) }
    }

    static func prepare(_ sql: String, db: OpaquePointer) throws -> OpaquePointer {
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK,
              let statement else { throw error(db) }
        return statement
    }

    static func stepDone(_ statement: OpaquePointer, _ db: OpaquePointer) throws {
        guard sqlite3_step(statement) == SQLITE_DONE else { throw error(db) }
    }

    static func bind(_ value: String?, _ index: Int32, _ statement: OpaquePointer, _ db: OpaquePointer) throws {
        guard let value else { return try bindNull(index, statement, db) }
        let destructor = unsafeBitCast(-1, to: sqlite3_destructor_type.self)
        guard sqlite3_bind_text(statement, index, value, -1, destructor) == SQLITE_OK else { throw error(db) }
    }

    static func bind(_ value: Int64, _ index: Int32, _ statement: OpaquePointer, _ db: OpaquePointer) throws {
        guard sqlite3_bind_int64(statement, index, value) == SQLITE_OK else { throw error(db) }
    }

    static func bind(_ value: Date?, _ index: Int32, _ statement: OpaquePointer, _ db: OpaquePointer) throws {
        guard let value else { return try bindNull(index, statement, db) }
        guard sqlite3_bind_double(statement, index, value.timeIntervalSince1970) == SQLITE_OK else { throw error(db) }
    }

    static func bind(_ value: Data?, _ index: Int32, _ statement: OpaquePointer, _ db: OpaquePointer) throws {
        guard let value else { return try bindNull(index, statement, db) }
        let result = value.withUnsafeBytes { bytes in
            sqlite3_bind_blob(statement, index, bytes.baseAddress, Int32(bytes.count), unsafeBitCast(-1, to: sqlite3_destructor_type.self))
        }
        guard result == SQLITE_OK else { throw error(db) }
    }

    static func text(_ statement: OpaquePointer, _ index: Int32) -> String? {
        sqlite3_column_text(statement, index).map { String(cString: $0) }
    }

    static func data(_ statement: OpaquePointer, _ index: Int32) -> Data? {
        guard let bytes = sqlite3_column_blob(statement, index) else { return nil }
        return Data(bytes: bytes, count: Int(sqlite3_column_bytes(statement, index)))
    }

    static func date(_ statement: OpaquePointer, _ index: Int32) -> Date? {
        guard sqlite3_column_type(statement, index) != SQLITE_NULL else { return nil }
        return Date(timeIntervalSince1970: sqlite3_column_double(statement, index))
    }

    static func tableExists(_ table: String, _ db: OpaquePointer) throws -> Bool {
        let statement = try prepare("SELECT 1 FROM sqlite_master WHERE type='table' AND name=?", db: db)
        defer { sqlite3_finalize(statement) }
        try bind(table, 1, statement, db)
        return sqlite3_step(statement) == SQLITE_ROW
    }

    static func columnExists(_ column: String, table: String, _ db: OpaquePointer) throws -> Bool {
        let statement = try prepare("PRAGMA table_info(\(table))", db: db)
        defer { sqlite3_finalize(statement) }
        while sqlite3_step(statement) == SQLITE_ROW {
            if text(statement, 1) == column { return true }
        }
        return false
    }

    private static func bindNull(_ index: Int32, _ statement: OpaquePointer, _ db: OpaquePointer) throws {
        guard sqlite3_bind_null(statement, index) == SQLITE_OK else { throw error(db) }
    }

    private static func error(_ db: OpaquePointer) -> JobStoreError {
        JobStoreError.sqlite(String(cString: sqlite3_errmsg(db)))
    }
}
