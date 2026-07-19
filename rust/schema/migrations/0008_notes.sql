CREATE TABLE pod0_note_imports(
    import_id BLOB PRIMARY KEY NOT NULL CHECK(length(import_id) = 16),
    source_kind INTEGER NOT NULL CHECK(source_kind IN (1, 2)),
    source_hash TEXT NOT NULL CHECK(length(source_hash) = 64),
    source_generation INTEGER NOT NULL CHECK(source_generation >= 0),
    note_count INTEGER NOT NULL CHECK(note_count >= 0),
    backup_byte_count INTEGER NOT NULL CHECK(backup_byte_count > 0),
    target_revision INTEGER NOT NULL CHECK(target_revision >= 1),
    state TEXT NOT NULL CHECK(state = 'verified'),
    verified_at_ms INTEGER NOT NULL,
    UNIQUE(source_kind, source_hash, source_generation)
) STRICT;

CREATE TABLE pod0_note_state(
    singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton = 1),
    collection_revision INTEGER NOT NULL CHECK(collection_revision >= 1),
    source_import_id BLOB NOT NULL REFERENCES pod0_note_imports(import_id)
) STRICT;

CREATE TABLE pod0_notes(
    note_id BLOB PRIMARY KEY NOT NULL CHECK(length(note_id) = 16),
    note_revision INTEGER NOT NULL CHECK(note_revision >= 1),
    text TEXT NOT NULL CHECK(length(CAST(text AS BLOB)) <= 65536),
    kind_code INTEGER NOT NULL CHECK(kind_code IN (1, 2, 3, 255)),
    kind_wire_code INTEGER,
    author_code INTEGER NOT NULL CHECK(author_code IN (1, 2, 255)),
    author_wire_code INTEGER,
    target_code INTEGER NOT NULL CHECK(target_code IN (0, 1, 2, 255)),
    target_wire_code INTEGER,
    target_note_id BLOB CHECK(target_note_id IS NULL OR length(target_note_id) = 16),
    episode_id BLOB CHECK(episode_id IS NULL OR length(episode_id) = 16),
    position_ms INTEGER CHECK(position_ms IS NULL OR position_ms >= 0),
    created_at_ms INTEGER NOT NULL,
    deleted INTEGER NOT NULL CHECK(deleted IN (0, 1)),
    evidence_generation_id BLOB
        CHECK(evidence_generation_id IS NULL OR length(evidence_generation_id) = 16),
    evidence_transcript_version_id BLOB
        CHECK(evidence_transcript_version_id IS NULL OR length(evidence_transcript_version_id) = 16),
    evidence_content_digest BLOB
        CHECK(evidence_content_digest IS NULL OR length(evidence_content_digest) = 32),
    evidence_span_id BLOB CHECK(evidence_span_id IS NULL OR length(evidence_span_id) = 16),
    source_import_id BLOB REFERENCES pod0_note_imports(import_id),
    created_command_id BLOB CHECK(created_command_id IS NULL OR length(created_command_id) = 16),
    CHECK((kind_code = 255) = (kind_wire_code IS NOT NULL)),
    CHECK((author_code = 255) = (author_wire_code IS NOT NULL)),
    CHECK((target_code = 255) = (target_wire_code IS NOT NULL)),
    CHECK((target_code = 1) = (target_note_id IS NOT NULL)),
    CHECK((target_code = 2) = (episode_id IS NOT NULL)),
    CHECK((target_code = 2) = (position_ms IS NOT NULL)),
    CHECK((source_import_id IS NULL) <> (created_command_id IS NULL)),
    CHECK(
        (evidence_generation_id IS NULL AND evidence_transcript_version_id IS NULL
            AND evidence_content_digest IS NULL AND evidence_span_id IS NULL)
        OR
        (evidence_generation_id IS NOT NULL AND evidence_transcript_version_id IS NOT NULL
            AND evidence_content_digest IS NOT NULL AND evidence_span_id IS NOT NULL)
    )
) STRICT;

CREATE INDEX pod0_notes_active_created_idx
ON pod0_notes(deleted, created_at_ms DESC, note_id);

CREATE INDEX pod0_notes_episode_position_idx
ON pod0_notes(episode_id, deleted, position_ms, note_id)
WHERE target_code = 2;
