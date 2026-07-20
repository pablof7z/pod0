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
}
