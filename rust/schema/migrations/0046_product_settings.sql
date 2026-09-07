CREATE TABLE pod0_product_settings(
    singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton=1),
    schema_version INTEGER NOT NULL CHECK(schema_version=1),
    revision INTEGER NOT NULL CHECK(revision>=0),
    writer_counter INTEGER NOT NULL CHECK(writer_counter>=0),
    writer_id BLOB NOT NULL CHECK(length(writer_id)=32),
    values_json TEXT NOT NULL CHECK(json_valid(values_json)),
    created_at_ms INTEGER NOT NULL CHECK(created_at_ms>=0),
    updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms>=created_at_ms)
) STRICT;

CREATE TABLE pod0_settings_validation_evidence(
    command_id BLOB PRIMARY KEY NOT NULL CHECK(length(command_id)=16),
    source_code INTEGER NOT NULL CHECK(source_code BETWEEN 0 AND 2),
    candidate_schema_version INTEGER NOT NULL CHECK(candidate_schema_version>=0),
    candidate_counter INTEGER NOT NULL CHECK(candidate_counter>=0),
    writer_id BLOB NOT NULL CHECK(length(writer_id)=32),
    validation_json TEXT NOT NULL CHECK(json_valid(validation_json)),
    observed_at_ms INTEGER NOT NULL CHECK(observed_at_ms>=0)
) STRICT;

CREATE TABLE pod0_settings_sync_conflicts(
    command_id BLOB PRIMARY KEY NOT NULL CHECK(length(command_id)=16),
    current_counter INTEGER NOT NULL CHECK(current_counter>=0),
    current_writer_id BLOB NOT NULL CHECK(length(current_writer_id)=32),
    candidate_counter INTEGER NOT NULL CHECK(candidate_counter>=0),
    candidate_writer_id BLOB NOT NULL CHECK(length(candidate_writer_id)=32),
    current_digest BLOB NOT NULL CHECK(length(current_digest)=32),
    candidate_digest BLOB NOT NULL CHECK(length(candidate_digest)=32),
    winner_code INTEGER NOT NULL CHECK(winner_code IN(0,1)),
    observed_at_ms INTEGER NOT NULL CHECK(observed_at_ms>=0)
) STRICT;

CREATE TRIGGER pod0_settings_validation_evidence_no_update
BEFORE UPDATE ON pod0_settings_validation_evidence
BEGIN
    SELECT RAISE(ABORT,'pod0 settings validation evidence is append-only');
END;

CREATE TRIGGER pod0_settings_validation_evidence_no_delete
BEFORE DELETE ON pod0_settings_validation_evidence
BEGIN
    SELECT RAISE(ABORT,'pod0 settings validation evidence is append-only');
END;

CREATE TRIGGER pod0_settings_sync_conflicts_no_update
BEFORE UPDATE ON pod0_settings_sync_conflicts
BEGIN
    SELECT RAISE(ABORT,'pod0 settings conflict evidence is append-only');
END;

CREATE TRIGGER pod0_settings_sync_conflicts_no_delete
BEFORE DELETE ON pod0_settings_sync_conflicts
BEGIN
    SELECT RAISE(ABORT,'pod0 settings conflict evidence is append-only');
END;
