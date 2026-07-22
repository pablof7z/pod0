extension JobStore {
    func removeAll() throws {
        try withDatabase { db in
            try WorkflowSQLite.execute("DELETE FROM jobs", db)
        }
    }

    func removeDerivableJobs() throws {
        try withDatabase { db in
            try WorkflowSQLite.execute(
                """
                DELETE FROM jobs WHERE occurrence_id IS NULL
                  AND kind IN (\(Self.supportedKindSQL))
                """,
                db
            )
        }
    }
}
