CREATE TABLE pod0_publications(
    publication_id BLOB PRIMARY KEY NOT NULL CHECK(length(publication_id)=16),
    artifact_id BLOB NOT NULL CHECK(length(artifact_id)=16),
    artifact_kind_code INTEGER NOT NULL,
    artifact_kind_wire_code INTEGER,
    episode_id BLOB NOT NULL CHECK(length(episode_id)=16),
    podcast_id BLOB NOT NULL CHECK(length(podcast_id)=16),
    semantic_revision INTEGER NOT NULL CHECK(semantic_revision BETWEEN 1 AND 4294967295),
    state_revision INTEGER NOT NULL CHECK(state_revision >= 1),
    expected_author_hex TEXT NOT NULL
        CHECK(length(expected_author_hex)=64 AND expected_author_hex=lower(expected_author_hex)),
    correlation_token TEXT UNIQUE NOT NULL
        CHECK(length(CAST(correlation_token AS BLOB)) BETWEEN 1 AND 64),
    public_media_url TEXT NOT NULL
        CHECK(length(CAST(public_media_url AS BLOB)) BETWEEN 8 AND 8192),
    media_type TEXT NOT NULL CHECK(media_type='audio/mpeg'),
    media_byte_count INTEGER NOT NULL CHECK(media_byte_count BETWEEN 1 AND 134217728),
    media_content_digest BLOB NOT NULL CHECK(length(media_content_digest)=32),
    receipt_id BLOB CHECK(receipt_id IS NULL OR length(receipt_id)=8),
    event_id_hex TEXT CHECK(event_id_hex IS NULL OR length(event_id_hex)=64),
    stage_code TEXT NOT NULL,
    prepared_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    UNIQUE(artifact_id,semantic_revision)
) STRICT;

CREATE TABLE pod0_publication_facts(
    publication_id BLOB NOT NULL
        REFERENCES pod0_publications(publication_id) ON DELETE CASCADE
        CHECK(length(publication_id)=16),
    sequence_number INTEGER NOT NULL CHECK(sequence_number >= 1),
    fact_digest BLOB NOT NULL CHECK(length(fact_digest)=32),
    fact_kind_code TEXT NOT NULL,
    route_id BLOB CHECK(route_id IS NULL OR length(route_id)=16),
    attempt BLOB CHECK(attempt IS NULL OR length(attempt)=8),
    event_id_hex TEXT CHECK(event_id_hex IS NULL OR length(event_id_hex)=64),
    observed_at_ms INTEGER,
    detail TEXT CHECK(detail IS NULL OR length(CAST(detail AS BLOB)) <= 512),
    PRIMARY KEY(publication_id,sequence_number),
    UNIQUE(publication_id,fact_digest)
) STRICT;

CREATE TABLE pod0_publication_commands(
    command_id BLOB PRIMARY KEY NOT NULL CHECK(length(command_id)=16),
    command_fingerprint TEXT NOT NULL CHECK(length(command_fingerprint)=64),
    publication_id BLOB NOT NULL
        REFERENCES pod0_publications(publication_id) ON DELETE CASCADE
        CHECK(length(publication_id)=16),
    completed_at_ms INTEGER NOT NULL
) STRICT;

CREATE INDEX pod0_publication_recovery_v1
    ON pod0_publications(stage_code,updated_at_ms,publication_id);
