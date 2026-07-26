import CSQLite3

extension WorkflowSchemaMigrations {
    static func ensureFeedDiscoveryRetirement(_ db: OpaquePointer) throws {
        try migrate(component: "feed_discovery_retirement", db: db) { recordedVersion in
            guard try WorkflowSQLite.tableExists(
                "legacy_feed_discovery_retirement",
                db
            ) else {
                guard recordedVersion == nil else {
                    throw unsupported(
                        "feed_discovery_retirement",
                        "version metadata exists but the table is missing"
                    )
                }
                try WorkflowSQLite.execute(
                    """
                    CREATE TABLE legacy_feed_discovery_retirement(
                        singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton=1),
                        schema_version INTEGER NOT NULL CHECK(schema_version=1),
                        source_generation INTEGER NOT NULL CHECK(source_generation>0),
                        source_digest TEXT NOT NULL CHECK(length(source_digest)=64),
                        retired_job_count INTEGER NOT NULL CHECK(retired_job_count>=0),
                        retired_artifact_count INTEGER NOT NULL CHECK(retired_artifact_count>=0),
                        completed_at REAL NOT NULL
                    )
                    """,
                    db
                )
                try setVersion(currentVersion, component: "feed_discovery_retirement", db: db)
                return
            }
            let columns = try columnNames(
                table: "legacy_feed_discovery_retirement",
                db: db
            )
            guard columns == feedDiscoveryRetirementColumns else {
                throw unsupported(
                    "feed_discovery_retirement",
                    "unrecognized columns: \(columns.sorted().joined(separator: ","))"
                )
            }
            if let recordedVersion, recordedVersion != currentVersion {
                throw unsupported(
                    "feed_discovery_retirement",
                    "recorded version is \(recordedVersion)"
                )
            }
            try setVersion(currentVersion, component: "feed_discovery_retirement", db: db)
        }
    }

    private static let feedDiscoveryRetirementColumns: Set<String> = [
        "singleton", "schema_version", "source_generation", "source_digest",
        "retired_job_count", "retired_artifact_count", "completed_at",
    ]
}
