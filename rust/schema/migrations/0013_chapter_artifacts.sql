CREATE TABLE pod0_chapter_imports(
    import_id BLOB PRIMARY KEY NOT NULL CHECK(length(import_id) = 16),
    source_kind TEXT NOT NULL CHECK(source_kind IN (
        'artifact_sqlite_v0', 'artifact_sqlite_v1'
    )),
    source_identity BLOB NOT NULL CHECK(length(source_identity) = 32),
    source_generation INTEGER NOT NULL CHECK(source_generation >= 0),
    source_byte_count INTEGER NOT NULL CHECK(source_byte_count >= 0),
    source_database_digest BLOB NOT NULL CHECK(length(source_database_digest) = 32),
    source_selection_digest BLOB NOT NULL CHECK(length(source_selection_digest) = 32),
    command_fingerprint BLOB NOT NULL UNIQUE CHECK(length(command_fingerprint) = 32),
    evidence_count INTEGER NOT NULL CHECK(evidence_count BETWEEN 0 AND 200000),
    artifact_count INTEGER NOT NULL CHECK(artifact_count BETWEEN 0 AND 50000),
    selected_count INTEGER NOT NULL CHECK(selected_count BETWEEN 0 AND evidence_count),
    blocked_count INTEGER NOT NULL CHECK(blocked_count BETWEEN 0 AND evidence_count),
    chapter_count INTEGER NOT NULL CHECK(chapter_count BETWEEN 0 AND 204800000),
    ad_span_count INTEGER NOT NULL CHECK(ad_span_count BETWEEN 0 AND 204800000),
    target_revision INTEGER NOT NULL CHECK(target_revision >= 1),
    state TEXT NOT NULL CHECK(state IN (
        'staged', 'verified', 'imported', 'corrupt', 'discarded'
    )),
    backup_database_digest BLOB NOT NULL CHECK(length(backup_database_digest) = 32),
    backup_database_byte_count INTEGER NOT NULL CHECK(backup_database_byte_count >= 0),
    backup_file_count INTEGER NOT NULL CHECK(backup_file_count BETWEEN 0 AND 200000),
    backup_file_byte_count INTEGER NOT NULL CHECK(backup_file_byte_count >= 0),
    staged_at_ms INTEGER NOT NULL CHECK(staged_at_ms >= 0),
    verified_at_ms INTEGER CHECK(verified_at_ms IS NULL OR verified_at_ms >= staged_at_ms),
    imported_at_ms INTEGER CHECK(imported_at_ms IS NULL OR imported_at_ms >= staged_at_ms),
    discarded_at_ms INTEGER CHECK(discarded_at_ms IS NULL OR discarded_at_ms >= staged_at_ms),
    diagnostic_code TEXT CHECK(
        diagnostic_code IS NULL
        OR length(CAST(diagnostic_code AS BLOB)) BETWEEN 1 AND 128
    ),
    CHECK(state NOT IN ('verified', 'imported') OR verified_at_ms IS NOT NULL),
    CHECK((state = 'imported') = (imported_at_ms IS NOT NULL)),
    CHECK((state = 'discarded') = (discarded_at_ms IS NOT NULL))
) STRICT;

CREATE INDEX pod0_chapter_imports_source_idx
ON pod0_chapter_imports(source_identity, source_generation);

CREATE TABLE pod0_chapter_artifacts(
    artifact_id BLOB PRIMARY KEY NOT NULL CHECK(length(artifact_id) = 16),
    schema_version INTEGER NOT NULL CHECK(schema_version >= 1),
    content_digest BLOB NOT NULL CHECK(length(content_digest) = 32),
    integrity_digest BLOB NOT NULL CHECK(length(integrity_digest) = 32),
    episode_id BLOB NOT NULL CHECK(length(episode_id) = 16),
    podcast_id BLOB NOT NULL CHECK(length(podcast_id) = 16),
    source_revision TEXT NOT NULL
        CHECK(length(CAST(source_revision AS BLOB)) BETWEEN 1 AND 256),
    source_code INTEGER NOT NULL CHECK(source_code IN (1, 2, 3, 4)),
    provider TEXT CHECK(
        provider IS NULL OR length(CAST(provider AS BLOB)) BETWEEN 1 AND 128
    ),
    model TEXT CHECK(model IS NULL OR length(CAST(model AS BLOB)) BETWEEN 1 AND 256),
    policy_version INTEGER NOT NULL CHECK(policy_version BETWEEN 0 AND 4294967295),
    source_payload_digest BLOB NOT NULL CHECK(length(source_payload_digest) = 32),
    transcript_version_id BLOB CHECK(
        transcript_version_id IS NULL OR length(transcript_version_id) = 16
    ),
    transcript_content_digest BLOB CHECK(
        transcript_content_digest IS NULL OR length(transcript_content_digest) = 32
    ),
    generated_at_ms INTEGER NOT NULL CHECK(generated_at_ms >= 0),
    duration_ms INTEGER CHECK(duration_ms IS NULL OR duration_ms > 0),
    chapter_count INTEGER NOT NULL CHECK(chapter_count BETWEEN 1 AND 4096),
    ad_span_evaluation_code INTEGER NOT NULL CHECK(ad_span_evaluation_code IN (1, 2)),
    ad_span_count INTEGER NOT NULL CHECK(ad_span_count BETWEEN 0 AND 4096),
    legacy_source_code INTEGER CHECK(legacy_source_code IS NULL OR legacy_source_code IN (1, 2, 3)),
    legacy_original_origin TEXT CHECK(
        legacy_original_origin IS NULL
        OR length(CAST(legacy_original_origin AS BLOB)) BETWEEN 1 AND 4096
    ),
    legacy_generated_at_was_unknown INTEGER CHECK(
        legacy_generated_at_was_unknown IS NULL
        OR legacy_generated_at_was_unknown IN (0, 1)
    ),
    source_import_id BLOB REFERENCES pod0_chapter_imports(import_id),
    created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
    CHECK((transcript_version_id IS NULL) = (transcript_content_digest IS NULL)),
    CHECK(ad_span_evaluation_code != 1 OR ad_span_count = 0),
    CHECK(
        (legacy_source_code IS NULL
            AND legacy_original_origin IS NULL
            AND legacy_generated_at_was_unknown IS NULL)
        OR
        (legacy_source_code IS NOT NULL
            AND legacy_generated_at_was_unknown IS NOT NULL)
    ),
    CHECK(legacy_generated_at_was_unknown != 1 OR generated_at_ms = 0),
    CHECK(
        legacy_source_code IS NOT NULL
        OR (source_code = 1 AND policy_version = 0 AND model IS NULL
            AND transcript_version_id IS NULL)
        OR (source_code IN (2, 3) AND policy_version > 0 AND provider IS NOT NULL
            AND model IS NOT NULL AND transcript_version_id IS NOT NULL)
        OR (source_code = 4 AND policy_version > 0)
    ),
    CHECK(legacy_source_code IS NULL OR source_code != 1 OR model IS NULL),
    UNIQUE(artifact_id, episode_id)
) STRICT;

CREATE INDEX pod0_chapter_artifacts_episode_idx
ON pod0_chapter_artifacts(episode_id, created_at_ms DESC);

CREATE INDEX pod0_chapter_artifacts_import_idx
ON pod0_chapter_artifacts(source_import_id);

CREATE TABLE pod0_chapter_items(
    chapter_id BLOB PRIMARY KEY NOT NULL CHECK(length(chapter_id) = 16),
    artifact_id BLOB NOT NULL
        REFERENCES pod0_chapter_artifacts(artifact_id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL CHECK(ordinal BETWEEN 0 AND 4095),
    start_ms INTEGER NOT NULL CHECK(start_ms >= 0),
    end_ms INTEGER CHECK(end_ms IS NULL OR end_ms > start_ms),
    title TEXT NOT NULL CHECK(length(CAST(title AS BLOB)) BETWEEN 1 AND 1024),
    summary TEXT CHECK(summary IS NULL OR length(CAST(summary AS BLOB)) BETWEEN 1 AND 16384),
    image_url TEXT CHECK(image_url IS NULL OR length(CAST(image_url AS BLOB)) BETWEEN 1 AND 4096),
    link_url TEXT CHECK(link_url IS NULL OR length(CAST(link_url AS BLOB)) BETWEEN 1 AND 4096),
    include_in_table_of_contents INTEGER NOT NULL
        CHECK(include_in_table_of_contents IN (0, 1)),
    source_episode_id BLOB CHECK(source_episode_id IS NULL OR length(source_episode_id) = 16),
    UNIQUE(artifact_id, ordinal),
    UNIQUE(artifact_id, chapter_id)
) STRICT;

CREATE TABLE pod0_ad_spans(
    ad_span_id BLOB PRIMARY KEY NOT NULL CHECK(length(ad_span_id) = 16),
    artifact_id BLOB NOT NULL
        REFERENCES pod0_chapter_artifacts(artifact_id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL CHECK(ordinal BETWEEN 0 AND 4095),
    start_ms INTEGER NOT NULL CHECK(start_ms >= 0),
    end_ms INTEGER NOT NULL CHECK(end_ms > start_ms),
    kind_code INTEGER NOT NULL CHECK(kind_code IN (1, 2, 3)),
    UNIQUE(artifact_id, ordinal),
    UNIQUE(artifact_id, ad_span_id)
) STRICT;

CREATE TABLE pod0_chapter_import_entries(
    import_id BLOB NOT NULL
        REFERENCES pod0_chapter_imports(import_id) ON DELETE CASCADE,
    entry_id BLOB NOT NULL CHECK(length(entry_id) = 32),
    evidence_kind TEXT NOT NULL CHECK(evidence_kind IN (
        'episode_adjunct', 'workflow_chapters', 'workflow_ad_spans',
        'attempt_manifest', 'unreferenced_chapter_file', 'unreferenced_ad_file'
    )),
    source_kind TEXT NOT NULL CHECK(source_kind IN (
        'episode_adjunct', 'workflow_artifact_v0', 'workflow_artifact_v1'
    )),
    source_subject TEXT NOT NULL
        CHECK(length(CAST(source_subject AS BLOB)) BETWEEN 1 AND 4096),
    source_input_version TEXT CHECK(
        source_input_version IS NULL
        OR length(CAST(source_input_version AS BLOB)) BETWEEN 1 AND 1024
    ),
    source_output_version TEXT CHECK(
        source_output_version IS NULL
        OR length(CAST(source_output_version AS BLOB)) BETWEEN 1 AND 1024
    ),
    source_origin TEXT CHECK(
        source_origin IS NULL OR length(CAST(source_origin AS BLOB)) BETWEEN 1 AND 4096
    ),
    source_schema_version INTEGER NOT NULL CHECK(source_schema_version >= 0),
    source_integrity TEXT NOT NULL
        CHECK(length(CAST(source_integrity AS BLOB)) BETWEEN 1 AND 128),
    source_verified_at_ms INTEGER CHECK(
        source_verified_at_ms IS NULL OR source_verified_at_ms >= 0
    ),
    episode_id BLOB CHECK(episode_id IS NULL OR length(episode_id) = 16),
    podcast_id BLOB CHECK(podcast_id IS NULL OR length(podcast_id) = 16),
    source_row_id INTEGER CHECK(source_row_id IS NULL OR source_row_id >= 0),
    source_row_digest BLOB NOT NULL CHECK(length(source_row_digest) = 32),
    source_file_path TEXT NOT NULL
        CHECK(length(CAST(source_file_path AS BLOB)) BETWEEN 1 AND 4096),
    source_file_digest BLOB NOT NULL CHECK(length(source_file_digest) = 32),
    source_file_byte_count INTEGER NOT NULL CHECK(source_file_byte_count >= 0),
    raw_digest BLOB NOT NULL CHECK(length(raw_digest) = 32),
    raw_byte_count INTEGER NOT NULL CHECK(raw_byte_count >= 0),
    backup_file_digest BLOB NOT NULL CHECK(length(backup_file_digest) = 32),
    backup_file_byte_count INTEGER NOT NULL CHECK(backup_file_byte_count >= 0),
    legacy_selected INTEGER CHECK(legacy_selected IS NULL OR legacy_selected IN (0, 1)),
    importer_selected INTEGER NOT NULL CHECK(importer_selected IN (0, 1)),
    validation_state TEXT NOT NULL CHECK(validation_state IN ('canonical', 'inert', 'blocked')),
    diagnostic_code TEXT CHECK(
        diagnostic_code IS NULL
        OR length(CAST(diagnostic_code AS BLOB)) BETWEEN 1 AND 128
    ),
    artifact_id BLOB CHECK(artifact_id IS NULL OR length(artifact_id) = 16),
    PRIMARY KEY(import_id, entry_id),
    CHECK((validation_state = 'canonical') = (artifact_id IS NOT NULL)),
    CHECK((validation_state = 'blocked') = (diagnostic_code IS NOT NULL)),
    CHECK(validation_state != 'inert' OR diagnostic_code IS NULL),
    FOREIGN KEY(artifact_id) REFERENCES pod0_chapter_artifacts(artifact_id)
) STRICT;

CREATE INDEX pod0_chapter_import_entries_selected_idx
ON pod0_chapter_import_entries(import_id, importer_selected, artifact_id);

CREATE TABLE pod0_chapter_import_chapter_evidence(
    import_id BLOB NOT NULL CHECK(length(import_id) = 16),
    entry_id BLOB NOT NULL CHECK(length(entry_id) = 32),
    ordinal INTEGER NOT NULL CHECK(ordinal BETWEEN 0 AND 4095),
    legacy_id BLOB CHECK(legacy_id IS NULL OR length(legacy_id) = 16),
    legacy_is_ai_generated INTEGER NOT NULL CHECK(legacy_is_ai_generated IN (0, 1)),
    chapter_id BLOB CHECK(chapter_id IS NULL OR length(chapter_id) = 16),
    PRIMARY KEY(import_id, entry_id, ordinal),
    FOREIGN KEY(import_id, entry_id)
        REFERENCES pod0_chapter_import_entries(import_id, entry_id) ON DELETE CASCADE,
    FOREIGN KEY(chapter_id) REFERENCES pod0_chapter_items(chapter_id)
) STRICT;

CREATE TABLE pod0_chapter_import_ad_evidence(
    import_id BLOB NOT NULL CHECK(length(import_id) = 16),
    entry_id BLOB NOT NULL CHECK(length(entry_id) = 32),
    ordinal INTEGER NOT NULL CHECK(ordinal BETWEEN 0 AND 4095),
    legacy_id BLOB CHECK(legacy_id IS NULL OR length(legacy_id) = 16),
    ad_span_id BLOB CHECK(ad_span_id IS NULL OR length(ad_span_id) = 16),
    PRIMARY KEY(import_id, entry_id, ordinal),
    FOREIGN KEY(import_id, entry_id)
        REFERENCES pod0_chapter_import_entries(import_id, entry_id) ON DELETE CASCADE,
    FOREIGN KEY(ad_span_id) REFERENCES pod0_ad_spans(ad_span_id)
) STRICT;

CREATE TABLE pod0_chapter_selections(
    episode_id BLOB NOT NULL CHECK(length(episode_id) = 16),
    selection_revision INTEGER NOT NULL CHECK(selection_revision >= 1),
    artifact_id BLOB NOT NULL CHECK(length(artifact_id) = 16),
    source_import_id BLOB NOT NULL CHECK(length(source_import_id) = 16),
    selected_at_ms INTEGER NOT NULL CHECK(selected_at_ms >= 0),
    PRIMARY KEY(episode_id, selection_revision),
    UNIQUE(episode_id, source_import_id),
    FOREIGN KEY(artifact_id, episode_id)
        REFERENCES pod0_chapter_artifacts(artifact_id, episode_id),
    FOREIGN KEY(source_import_id) REFERENCES pod0_chapter_imports(import_id)
) STRICT;

CREATE INDEX pod0_chapter_selections_import_idx
ON pod0_chapter_selections(source_import_id, selection_revision);

CREATE TABLE pod0_chapter_state(
    singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton = 1),
    collection_revision INTEGER NOT NULL CHECK(collection_revision >= 0),
    authority_active INTEGER NOT NULL DEFAULT 0 CHECK(authority_active = 0),
    authority_import_id BLOB REFERENCES pod0_chapter_imports(import_id),
    CHECK(authority_import_id IS NULL)
) STRICT;

INSERT INTO pod0_chapter_state(
    singleton, collection_revision, authority_active, authority_import_id
) VALUES(1, 0, 0, NULL);
