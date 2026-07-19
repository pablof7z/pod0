ALTER TABLE pod0_transcript_imports
ADD COLUMN artifact_count INTEGER NOT NULL DEFAULT 0
    CHECK(artifact_count BETWEEN 0 AND 50000);

UPDATE pod0_transcript_imports
SET artifact_count = selected_count;

CREATE TABLE pod0_transcript_import_entries_v12(
    import_id BLOB NOT NULL
        REFERENCES pod0_transcript_imports(import_id) ON DELETE CASCADE,
    episode_id BLOB NOT NULL REFERENCES pod0_episodes(episode_id),
    legacy_row_id INTEGER NOT NULL CHECK(legacy_row_id >= 0),
    legacy_schema_version INTEGER NOT NULL CHECK(legacy_schema_version >= 0),
    legacy_input_version TEXT NOT NULL
        CHECK(length(CAST(legacy_input_version AS BLOB)) BETWEEN 1 AND 1024),
    legacy_output_version TEXT NOT NULL
        CHECK(length(CAST(legacy_output_version AS BLOB)) BETWEEN 1 AND 1024),
    legacy_origin TEXT NOT NULL
        CHECK(length(CAST(legacy_origin AS BLOB)) BETWEEN 1 AND 128),
    legacy_integrity TEXT NOT NULL
        CHECK(length(CAST(legacy_integrity AS BLOB)) BETWEEN 1 AND 128),
    legacy_verified_at_ms INTEGER NOT NULL CHECK(legacy_verified_at_ms >= 0),
    is_selected INTEGER NOT NULL CHECK(is_selected IN (0, 1)),
    selected_row_digest BLOB NOT NULL CHECK(length(selected_row_digest) = 32),
    selected_file_digest BLOB NOT NULL CHECK(length(selected_file_digest) = 32),
    backup_file_digest BLOB NOT NULL CHECK(length(backup_file_digest) = 32),
    backup_file_byte_count INTEGER NOT NULL CHECK(backup_file_byte_count >= 0),
    artifact_id BLOB NOT NULL,
    transcript_version_id BLOB NOT NULL,
    PRIMARY KEY(import_id, legacy_row_id),
    FOREIGN KEY(artifact_id, transcript_version_id)
        REFERENCES pod0_transcript_artifacts(artifact_id, transcript_version_id)
) STRICT;

INSERT INTO pod0_transcript_import_entries_v12(
    import_id,episode_id,legacy_row_id,legacy_schema_version,legacy_input_version,
    legacy_output_version,legacy_origin,legacy_integrity,legacy_verified_at_ms,
    is_selected,selected_row_digest,selected_file_digest,backup_file_digest,
    backup_file_byte_count,artifact_id,transcript_version_id
)
SELECT import_id,episode_id,legacy_row_id,legacy_schema_version,legacy_input_version,
       legacy_output_version,legacy_origin,legacy_integrity,legacy_verified_at_ms,
       1,selected_row_digest,selected_file_digest,backup_file_digest,
       backup_file_byte_count,artifact_id,transcript_version_id
FROM pod0_transcript_import_entries;

DROP TABLE pod0_transcript_import_entries;
ALTER TABLE pod0_transcript_import_entries_v12
RENAME TO pod0_transcript_import_entries;
