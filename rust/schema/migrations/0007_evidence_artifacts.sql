CREATE TABLE pod0_transcript_documents(
    transcript_version_id BLOB PRIMARY KEY NOT NULL CHECK(length(transcript_version_id) = 16),
    episode_id BLOB NOT NULL REFERENCES pod0_episodes(episode_id) ON DELETE CASCADE,
    podcast_id BLOB NOT NULL REFERENCES pod0_podcasts(podcast_id) ON DELETE CASCADE,
    source_revision TEXT NOT NULL
        CHECK(length(CAST(source_revision AS BLOB)) BETWEEN 1 AND 256),
    content_digest BLOB NOT NULL CHECK(length(content_digest) = 32),
    source_code INTEGER NOT NULL CHECK(source_code IN (1, 2, 3, 4, 5, 6, 255)),
    source_wire_code INTEGER,
    provider TEXT CHECK(length(CAST(provider AS BLOB)) BETWEEN 1 AND 128),
    source_payload_digest BLOB NOT NULL CHECK(length(source_payload_digest) = 32),
    segment_count INTEGER NOT NULL CHECK(segment_count BETWEEN 0 AND 50000),
    CHECK((source_code = 255) = (source_wire_code IS NOT NULL)),
    UNIQUE(transcript_version_id, episode_id)
) STRICT;

CREATE INDEX pod0_transcript_documents_episode_idx
ON pod0_transcript_documents(episode_id);

CREATE TABLE pod0_transcript_segments(
    segment_id BLOB PRIMARY KEY NOT NULL CHECK(length(segment_id) = 16),
    transcript_version_id BLOB NOT NULL
        REFERENCES pod0_transcript_documents(transcript_version_id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL CHECK(ordinal BETWEEN 0 AND 49999),
    text TEXT NOT NULL CHECK(length(CAST(text AS BLOB)) BETWEEN 1 AND 16384),
    start_ms INTEGER NOT NULL CHECK(start_ms >= 0),
    end_ms INTEGER NOT NULL CHECK(end_ms >= start_ms),
    speaker_id BLOB CHECK(speaker_id IS NULL OR length(speaker_id) = 16),
    UNIQUE(transcript_version_id, ordinal)
) STRICT;

CREATE TABLE pod0_evidence_generations(
    generation_id BLOB PRIMARY KEY NOT NULL CHECK(length(generation_id) = 16),
    transcript_version_id BLOB NOT NULL,
    episode_id BLOB NOT NULL,
    artifact_schema_version INTEGER NOT NULL CHECK(artifact_schema_version >= 1),
    integrity_digest BLOB NOT NULL CHECK(length(integrity_digest) = 32),
    chunk_policy_version INTEGER NOT NULL CHECK(chunk_policy_version >= 1),
    target_tokens INTEGER NOT NULL CHECK(target_tokens BETWEEN 20 AND 4096),
    overlap_per_mille INTEGER NOT NULL CHECK(overlap_per_mille BETWEEN 0 AND 500),
    snap_tolerance_per_mille INTEGER NOT NULL
        CHECK(snap_tolerance_per_mille BETWEEN 0 AND 500),
    span_count INTEGER NOT NULL CHECK(span_count BETWEEN 0 AND 50000),
    state TEXT NOT NULL CHECK(state IN ('staged', 'verified')),
    staged_at_ms INTEGER NOT NULL,
    verified_at_ms INTEGER,
    CHECK((state = 'verified') = (verified_at_ms IS NOT NULL)),
    FOREIGN KEY(transcript_version_id, episode_id)
        REFERENCES pod0_transcript_documents(transcript_version_id, episode_id)
        ON DELETE CASCADE,
    UNIQUE(generation_id, episode_id, state)
) STRICT;

CREATE INDEX pod0_evidence_generations_transcript_idx
ON pod0_evidence_generations(transcript_version_id);

CREATE TABLE pod0_evidence_spans(
    span_id BLOB PRIMARY KEY NOT NULL CHECK(length(span_id) = 16),
    generation_id BLOB NOT NULL
        REFERENCES pod0_evidence_generations(generation_id) ON DELETE CASCADE,
    sort_order INTEGER NOT NULL CHECK(sort_order BETWEEN 0 AND 49999),
    first_segment_id BLOB NOT NULL REFERENCES pod0_transcript_segments(segment_id),
    last_segment_id BLOB NOT NULL REFERENCES pod0_transcript_segments(segment_id),
    start_segment_ordinal INTEGER NOT NULL CHECK(start_segment_ordinal BETWEEN 0 AND 49999),
    end_segment_ordinal_exclusive INTEGER NOT NULL
        CHECK(end_segment_ordinal_exclusive BETWEEN 1 AND 50000),
    start_ms INTEGER NOT NULL CHECK(start_ms >= 0),
    end_ms INTEGER NOT NULL CHECK(end_ms >= start_ms),
    text TEXT NOT NULL CHECK(length(CAST(text AS BLOB)) BETWEEN 1 AND 65536),
    speaker_id BLOB CHECK(speaker_id IS NULL OR length(speaker_id) = 16),
    chunk_policy_version INTEGER NOT NULL CHECK(chunk_policy_version >= 1),
    CHECK(end_segment_ordinal_exclusive > start_segment_ordinal),
    UNIQUE(generation_id, sort_order)
) STRICT;

CREATE TABLE pod0_evidence_selection(
    episode_id BLOB PRIMARY KEY NOT NULL
        REFERENCES pod0_episodes(episode_id) ON DELETE CASCADE,
    generation_id BLOB NOT NULL,
    generation_state TEXT NOT NULL DEFAULT 'verified' CHECK(generation_state = 'verified'),
    selected_at_ms INTEGER NOT NULL,
    FOREIGN KEY(generation_id, episode_id, generation_state)
        REFERENCES pod0_evidence_generations(generation_id, episode_id, state)
) STRICT;

CREATE TABLE pod0_evidence_commands(
    command_id BLOB PRIMARY KEY NOT NULL CHECK(length(command_id) = 16),
    operation_code INTEGER NOT NULL CHECK(operation_code IN (1, 2, 3, 4)),
    command_fingerprint BLOB NOT NULL CHECK(length(command_fingerprint) = 32),
    generation_id BLOB NOT NULL CHECK(length(generation_id) = 16),
    episode_id BLOB CHECK(episode_id IS NULL OR length(episode_id) = 16),
    previous_generation_id BLOB
        CHECK(previous_generation_id IS NULL OR length(previous_generation_id) = 16),
    result_code INTEGER NOT NULL CHECK(result_code IN (0, 1)),
    completed_at_ms INTEGER NOT NULL,
    CHECK((operation_code = 3) = (episode_id IS NOT NULL)),
    CHECK(previous_generation_id IS NULL OR operation_code = 3)
) STRICT;
