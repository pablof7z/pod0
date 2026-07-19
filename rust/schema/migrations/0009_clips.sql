CREATE TABLE pod0_clip_imports(
    import_id BLOB PRIMARY KEY NOT NULL CHECK(length(import_id) = 16),
    source_kind INTEGER NOT NULL CHECK(source_kind IN (1, 2)),
    source_hash TEXT NOT NULL CHECK(length(source_hash) = 64),
    source_generation INTEGER NOT NULL CHECK(source_generation >= 0),
    clip_count INTEGER NOT NULL CHECK(clip_count >= 0),
    backup_byte_count INTEGER NOT NULL CHECK(backup_byte_count > 0),
    target_revision INTEGER NOT NULL CHECK(target_revision >= 1),
    state TEXT NOT NULL CHECK(state = 'verified'),
    verified_at_ms INTEGER NOT NULL,
    UNIQUE(source_kind, source_hash, source_generation)
) STRICT;

CREATE TABLE pod0_clip_state(
    singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton = 1),
    collection_revision INTEGER NOT NULL CHECK(collection_revision >= 1),
    source_import_id BLOB NOT NULL REFERENCES pod0_clip_imports(import_id)
) STRICT;

CREATE TABLE pod0_clips(
    clip_id BLOB PRIMARY KEY NOT NULL CHECK(length(clip_id) = 16),
    clip_revision INTEGER NOT NULL CHECK(clip_revision >= 1),
    episode_id BLOB NOT NULL CHECK(length(episode_id) = 16),
    podcast_id BLOB NOT NULL CHECK(length(podcast_id) = 16),
    start_ms INTEGER NOT NULL CHECK(start_ms >= 0),
    end_ms INTEGER NOT NULL CHECK(end_ms > start_ms),
    created_at_ms INTEGER NOT NULL,
    caption TEXT CHECK(caption IS NULL OR length(CAST(caption AS BLOB)) <= 4096),
    speaker_id BLOB CHECK(speaker_id IS NULL OR length(speaker_id) = 16),
    speaker_label TEXT,
    frozen_transcript_text TEXT NOT NULL
        CHECK(length(CAST(frozen_transcript_text AS BLOB)) <= 65536),
    source_code INTEGER NOT NULL CHECK(source_code IN (1, 2, 3, 4, 5, 6, 7, 255)),
    source_wire_code INTEGER,
    deleted INTEGER NOT NULL CHECK(deleted IN (0, 1)),
    evidence_generation_id BLOB
        CHECK(evidence_generation_id IS NULL OR length(evidence_generation_id) = 16),
    evidence_transcript_version_id BLOB
        CHECK(evidence_transcript_version_id IS NULL OR length(evidence_transcript_version_id) = 16),
    evidence_content_digest BLOB
        CHECK(evidence_content_digest IS NULL OR length(evidence_content_digest) = 32),
    evidence_span_id BLOB CHECK(evidence_span_id IS NULL OR length(evidence_span_id) = 16),
    source_import_id BLOB REFERENCES pod0_clip_imports(import_id),
    created_command_id BLOB CHECK(created_command_id IS NULL OR length(created_command_id) = 16),
    CHECK((source_code = 255) = (source_wire_code IS NOT NULL)),
    CHECK(speaker_id IS NULL OR speaker_label IS NULL),
    CHECK((source_import_id IS NULL) <> (created_command_id IS NULL)),
    CHECK(
        (evidence_generation_id IS NULL AND evidence_transcript_version_id IS NULL
            AND evidence_content_digest IS NULL AND evidence_span_id IS NULL)
        OR
        (evidence_generation_id IS NOT NULL AND evidence_transcript_version_id IS NOT NULL
            AND evidence_content_digest IS NOT NULL AND evidence_span_id IS NOT NULL)
    )
) STRICT;

CREATE INDEX pod0_clips_active_created_idx
ON pod0_clips(deleted, created_at_ms DESC, clip_id);

CREATE INDEX pod0_clips_episode_created_idx
ON pod0_clips(episode_id, deleted, created_at_ms DESC, clip_id);
