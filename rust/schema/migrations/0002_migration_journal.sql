CREATE TABLE pod0_migration_journal(
    migration_id BLOB NOT NULL CHECK(length(migration_id) = 16),
    from_version INTEGER NOT NULL,
    to_version INTEGER NOT NULL,
    state TEXT NOT NULL CHECK(state IN ('running', 'completed', 'failed')),
    started_at_ms INTEGER NOT NULL,
    completed_at_ms INTEGER,
    diagnostic_code TEXT,
    PRIMARY KEY(migration_id, to_version)
) STRICT;

CREATE TABLE pod0_backup_evidence(
    migration_id BLOB PRIMARY KEY NOT NULL CHECK(length(migration_id) = 16),
    store_id BLOB NOT NULL CHECK(length(store_id) = 16),
    schema_version INTEGER NOT NULL,
    byte_count INTEGER NOT NULL,
    page_count INTEGER NOT NULL,
    integrity_check TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL
) STRICT;
