import CSQLite3

extension JobStore {
    func removeJobs(kind: WorkJobKind) throws {
        try withDatabase { db in
            let statement = try WorkflowSQLite.prepare(
                "DELETE FROM jobs WHERE kind=?",
                db: db
            )
            defer { sqlite3_finalize(statement) }
            try WorkflowSQLite.bind(kind.rawValue, 1, statement, db)
            try WorkflowSQLite.stepDone(statement, db)
        }
    }

    /// Deletes one legacy workflow kind only if every persisted row still
    /// matches the verified snapshot. The comparison and delete share an
    /// immediate transaction so a concurrent writer can never be swept into
    /// an earlier migration manifest.
    func removeJobs(kind: WorkJobKind, matching expected: [WorkJob]) throws -> Bool {
        try withDatabase { db in
            try WorkflowSQLite.execute("BEGIN IMMEDIATE TRANSACTION", db)
            do {
                let current: [WorkJob]
                do {
                    let select = try WorkflowSQLite.prepare(
                        "SELECT \(Self.columns) FROM jobs WHERE kind=? ORDER BY id",
                        db: db
                    )
                    defer { sqlite3_finalize(select) }
                    try WorkflowSQLite.bind(kind.rawValue, 1, select, db)
                    current = try readRows(select)
                }
                guard current == expected.sorted(by: {
                    $0.id.uuidString < $1.id.uuidString
                }) else {
                    try WorkflowSQLite.execute("COMMIT TRANSACTION", db)
                    return false
                }

                let delete = try WorkflowSQLite.prepare(
                    "DELETE FROM jobs WHERE kind=?",
                    db: db
                )
                defer { sqlite3_finalize(delete) }
                try WorkflowSQLite.bind(kind.rawValue, 1, delete, db)
                try WorkflowSQLite.stepDone(delete, db)
                guard Int(sqlite3_changes(db)) == current.count else {
                    throw JobStoreError.transitionRejected
                }
                try WorkflowSQLite.execute("COMMIT TRANSACTION", db)
                return true
            } catch {
                try? WorkflowSQLite.execute("ROLLBACK TRANSACTION", db)
                throw error
            }
        }
    }
}
