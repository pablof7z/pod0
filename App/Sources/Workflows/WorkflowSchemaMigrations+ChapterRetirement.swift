import CSQLite3

extension WorkflowSchemaMigrations {
    static func ensureChapterRetirement(_ db: OpaquePointer) throws {
        try migrate(component: "chapter_retirement", db: db) { recordedVersion in
            guard try WorkflowSQLite.tableExists(
                "legacy_chapter_workflow_retirement",
                db
            ) else {
                guard recordedVersion == nil else {
                    throw unsupported(
                        "chapter_retirement",
                        "version metadata exists but the table is missing"
                    )
                }
                try WorkflowSQLite.execute(
                    """
                    CREATE TABLE legacy_chapter_workflow_retirement(
                        singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton=1),
                        schema_version INTEGER NOT NULL,
                        model_source_generation INTEGER NOT NULL,
                        publisher_source_generation INTEGER NOT NULL,
                        publisher_source_fingerprint TEXT NOT NULL,
                        completed_at REAL NOT NULL
                    )
                    """,
                    db
                )
                try setVersion(currentVersion, component: "chapter_retirement", db: db)
                return
            }
            let columns = try columnNames(
                table: "legacy_chapter_workflow_retirement",
                db: db
            )
            guard columns == chapterRetirementColumns else {
                throw unsupported(
                    "chapter_retirement",
                    "unrecognized columns: \(columns.sorted().joined(separator: ","))"
                )
            }
            if let recordedVersion, recordedVersion != currentVersion {
                throw unsupported(
                    "chapter_retirement",
                    "recorded version is \(recordedVersion)"
                )
            }
            try setVersion(currentVersion, component: "chapter_retirement", db: db)
        }
    }

    private static let chapterRetirementColumns: Set<String> = [
        "singleton", "schema_version", "model_source_generation",
        "publisher_source_generation", "publisher_source_fingerprint",
        "completed_at",
    ]
}
