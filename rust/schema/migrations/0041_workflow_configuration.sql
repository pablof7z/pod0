-- Workflow admission policy is absent until the one-time typed Swift import commits.
-- Absence is intentionally distinct from defaults and causes reconciliation to fail closed.
CREATE TABLE pod0_workflow_configuration(
    singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton=1),
    schema_version INTEGER NOT NULL CHECK(schema_version=1),
    authority_state TEXT NOT NULL CHECK(authority_state='authoritative'),
    origin TEXT NOT NULL CHECK(origin IN('legacy_swift_import','user')),
    revision INTEGER NOT NULL CHECK(revision>=0),
    configuration_json TEXT NOT NULL CHECK(json_valid(configuration_json)),
    source_generation BLOB CHECK(source_generation IS NULL OR length(source_generation)=32),
    created_at_ms INTEGER NOT NULL CHECK(created_at_ms>=0),
    updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms>=created_at_ms)
) STRICT;

-- This is availability only: the typed payload contains no credential, token, or local path.
CREATE TABLE pod0_workflow_capability_snapshot(
    singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton=1),
    schema_version INTEGER NOT NULL CHECK(schema_version=1),
    snapshot_id BLOB UNIQUE NOT NULL CHECK(length(snapshot_id)=32),
    revision INTEGER NOT NULL CHECK(revision>=0),
    snapshot_json TEXT NOT NULL CHECK(json_valid(snapshot_json)),
    observed_at_ms INTEGER NOT NULL CHECK(observed_at_ms>=0)
) STRICT;
