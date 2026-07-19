CREATE TABLE pod0_transcript_imports(
    import_id BLOB PRIMARY KEY NOT NULL CHECK(length(import_id) = 16),
    source_kind TEXT NOT NULL
        CHECK(source_kind IN ('artifact_sqlite_v0', 'artifact_sqlite_v1')),
    source_schema_version INTEGER NOT NULL CHECK(source_schema_version BETWEEN 0 AND 1),
    source_generation INTEGER NOT NULL CHECK(source_generation >= 0),
    source_selection_digest BLOB NOT NULL CHECK(length(source_selection_digest) = 32),
    source_database_digest BLOB NOT NULL CHECK(length(source_database_digest) = 32),
    backup_database_digest BLOB NOT NULL CHECK(length(backup_database_digest) = 32),
    backup_database_byte_count INTEGER NOT NULL CHECK(backup_database_byte_count >= 0),
    selected_count INTEGER NOT NULL CHECK(selected_count BETWEEN 0 AND 50000),
    target_revision INTEGER NOT NULL CHECK(target_revision >= 0),
    state TEXT NOT NULL
        CHECK(state IN ('staged', 'verified', 'committed', 'corrupt', 'discarded')),
    diagnostic_code TEXT
        CHECK(diagnostic_code IS NULL OR length(CAST(diagnostic_code AS BLOB)) BETWEEN 1 AND 128),
    staged_at_ms INTEGER NOT NULL CHECK(staged_at_ms >= 0),
    verified_at_ms INTEGER CHECK(verified_at_ms IS NULL OR verified_at_ms >= staged_at_ms),
    committed_at_ms INTEGER CHECK(committed_at_ms IS NULL OR committed_at_ms >= staged_at_ms),
    discarded_at_ms INTEGER CHECK(discarded_at_ms IS NULL OR discarded_at_ms >= staged_at_ms),
    CHECK((state IN ('verified', 'committed')) = (verified_at_ms IS NOT NULL)),
    CHECK((state = 'committed') = (committed_at_ms IS NOT NULL)),
    CHECK((state = 'discarded') = (discarded_at_ms IS NOT NULL))
) STRICT;

CREATE TABLE pod0_transcript_artifacts(
    artifact_id BLOB PRIMARY KEY NOT NULL CHECK(length(artifact_id) = 16),
    transcript_version_id BLOB NOT NULL,
    episode_id BLOB NOT NULL,
    schema_version INTEGER NOT NULL CHECK(schema_version >= 1),
    integrity_digest BLOB NOT NULL CHECK(length(integrity_digest) = 32),
    language TEXT NOT NULL CHECK(length(CAST(language AS BLOB)) BETWEEN 1 AND 64),
    generated_at_ms INTEGER NOT NULL CHECK(generated_at_ms >= 0),
    speaker_count INTEGER NOT NULL CHECK(speaker_count BETWEEN 0 AND 4096),
    segment_count INTEGER NOT NULL CHECK(segment_count BETWEEN 0 AND 50000),
    word_count INTEGER NOT NULL CHECK(word_count BETWEEN 0 AND 2000000),
    source_import_id BLOB REFERENCES pod0_transcript_imports(import_id),
    created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
    FOREIGN KEY(transcript_version_id, episode_id)
        REFERENCES pod0_transcript_documents(transcript_version_id, episode_id),
    UNIQUE(artifact_id, transcript_version_id),
    UNIQUE(artifact_id, episode_id)
) STRICT;

CREATE INDEX pod0_transcript_artifacts_episode_idx
ON pod0_transcript_artifacts(episode_id);

CREATE UNIQUE INDEX pod0_transcript_segments_identity_version_idx
ON pod0_transcript_segments(segment_id, transcript_version_id);

CREATE TABLE pod0_transcript_speakers(
    artifact_id BLOB NOT NULL
        REFERENCES pod0_transcript_artifacts(artifact_id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL CHECK(ordinal BETWEEN 0 AND 4095),
    speaker_id BLOB NOT NULL CHECK(length(speaker_id) = 16),
    label TEXT NOT NULL CHECK(length(CAST(label AS BLOB)) BETWEEN 1 AND 1024),
    display_name TEXT
        CHECK(display_name IS NULL OR length(CAST(display_name AS BLOB)) BETWEEN 1 AND 1024),
    PRIMARY KEY(artifact_id, ordinal),
    UNIQUE(artifact_id, speaker_id)
) STRICT;

CREATE TABLE pod0_transcript_artifact_segments(
    artifact_id BLOB NOT NULL,
    transcript_version_id BLOB NOT NULL,
    segment_id BLOB NOT NULL CHECK(length(segment_id) = 16),
    ordinal INTEGER NOT NULL CHECK(ordinal BETWEEN 0 AND 49999),
    raw_text TEXT NOT NULL CHECK(length(CAST(raw_text AS BLOB)) BETWEEN 1 AND 16384),
    word_count INTEGER NOT NULL CHECK(word_count BETWEEN 0 AND 2000000),
    PRIMARY KEY(artifact_id, segment_id),
    UNIQUE(artifact_id, ordinal),
    UNIQUE(artifact_id, segment_id, transcript_version_id),
    FOREIGN KEY(artifact_id, transcript_version_id)
        REFERENCES pod0_transcript_artifacts(artifact_id, transcript_version_id)
        ON DELETE CASCADE,
    FOREIGN KEY(segment_id, transcript_version_id)
        REFERENCES pod0_transcript_segments(segment_id, transcript_version_id)
) STRICT;

CREATE TABLE pod0_transcript_words(
    artifact_id BLOB NOT NULL,
    transcript_version_id BLOB NOT NULL,
    segment_id BLOB NOT NULL,
    ordinal INTEGER NOT NULL CHECK(ordinal BETWEEN 0 AND 1999999),
    text TEXT NOT NULL CHECK(length(CAST(text AS BLOB)) BETWEEN 1 AND 1024),
    start_ms INTEGER NOT NULL CHECK(start_ms >= 0),
    end_ms INTEGER NOT NULL CHECK(end_ms >= start_ms),
    PRIMARY KEY(artifact_id, segment_id, ordinal),
    FOREIGN KEY(artifact_id, segment_id, transcript_version_id)
        REFERENCES pod0_transcript_artifact_segments(
            artifact_id, segment_id, transcript_version_id
        ) ON DELETE CASCADE
) STRICT;

CREATE TABLE pod0_transcript_import_entries(
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
    selected_row_digest BLOB NOT NULL CHECK(length(selected_row_digest) = 32),
    selected_file_digest BLOB NOT NULL CHECK(length(selected_file_digest) = 32),
    backup_file_digest BLOB NOT NULL CHECK(length(backup_file_digest) = 32),
    backup_file_byte_count INTEGER NOT NULL CHECK(backup_file_byte_count >= 0),
    artifact_id BLOB NOT NULL,
    transcript_version_id BLOB NOT NULL,
    PRIMARY KEY(import_id, episode_id),
    UNIQUE(import_id, legacy_row_id),
    FOREIGN KEY(artifact_id, transcript_version_id)
        REFERENCES pod0_transcript_artifacts(artifact_id, transcript_version_id)
) STRICT;

CREATE TABLE pod0_transcript_selection(
    episode_id BLOB PRIMARY KEY NOT NULL
        REFERENCES pod0_episodes(episode_id) ON DELETE CASCADE,
    artifact_id BLOB NOT NULL,
    transcript_version_id BLOB NOT NULL,
    selection_revision INTEGER NOT NULL CHECK(selection_revision >= 1),
    selected_at_ms INTEGER NOT NULL CHECK(selected_at_ms >= 0),
    source_import_id BLOB REFERENCES pod0_transcript_imports(import_id),
    FOREIGN KEY(artifact_id, episode_id)
        REFERENCES pod0_transcript_artifacts(artifact_id, episode_id),
    FOREIGN KEY(artifact_id, transcript_version_id)
        REFERENCES pod0_transcript_artifacts(artifact_id, transcript_version_id)
) STRICT;

CREATE TABLE pod0_transcript_state(
    singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton = 1),
    collection_revision INTEGER NOT NULL CHECK(collection_revision >= 0),
    source_import_id BLOB REFERENCES pod0_transcript_imports(import_id)
) STRICT;

INSERT INTO pod0_transcript_state(singleton, collection_revision, source_import_id)
VALUES(1, 0, NULL);

CREATE TABLE pod0_transcript_commands(
    command_id BLOB PRIMARY KEY NOT NULL CHECK(length(command_id) = 16),
    operation_code INTEGER NOT NULL CHECK(operation_code = 1),
    command_fingerprint BLOB NOT NULL CHECK(length(command_fingerprint) = 32),
    episode_id BLOB NOT NULL CHECK(length(episode_id) = 16),
    artifact_id BLOB NOT NULL CHECK(length(artifact_id) = 16),
    transcript_version_id BLOB NOT NULL CHECK(length(transcript_version_id) = 16),
    expected_selection_revision INTEGER NOT NULL CHECK(expected_selection_revision >= 0),
    previous_artifact_id BLOB
        CHECK(previous_artifact_id IS NULL OR length(previous_artifact_id) = 16),
    resulting_selection_revision INTEGER NOT NULL CHECK(resulting_selection_revision >= 1),
    already_selected INTEGER NOT NULL CHECK(already_selected IN (0, 1)),
    completed_at_ms INTEGER NOT NULL CHECK(completed_at_ms >= 0),
    FOREIGN KEY(artifact_id, transcript_version_id)
        REFERENCES pod0_transcript_artifacts(artifact_id, transcript_version_id)
) STRICT;
