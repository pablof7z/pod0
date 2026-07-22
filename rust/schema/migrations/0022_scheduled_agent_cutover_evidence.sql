CREATE TABLE pod0_scheduled_agent_cutover_evidence(
    singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton=1),
    state TEXT NOT NULL CHECK(state IN('staged','verified','authoritative')),
    source_generation INTEGER NOT NULL CHECK(source_generation>=1),
    source_fingerprint BLOB NOT NULL CHECK(length(source_fingerprint)=32),
    backup_digest BLOB NOT NULL CHECK(length(backup_digest)=32),
    backup_byte_count INTEGER NOT NULL CHECK(backup_byte_count>=1),
    task_count INTEGER NOT NULL CHECK(task_count>=0),
    occurrence_count INTEGER NOT NULL CHECK(occurrence_count>=0),
    staged_at_ms INTEGER NOT NULL,
    verified_at_ms INTEGER,
    committed_at_ms INTEGER,
    CHECK((state='staged')=(verified_at_ms IS NULL AND committed_at_ms IS NULL)),
    CHECK((state='verified')=(verified_at_ms IS NOT NULL AND committed_at_ms IS NULL)),
    CHECK((state='authoritative')=(verified_at_ms IS NOT NULL AND committed_at_ms IS NOT NULL))
) STRICT;
