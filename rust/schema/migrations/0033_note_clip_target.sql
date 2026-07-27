-- Adds NoteTarget::Clip (target_code 3) — a note about a clip as an artifact.
--
-- This is a table rebuild rather than an ALTER TABLE ADD COLUMN. The new column
-- alone would be additive, but target_code carries
-- CHECK(target_code IN (0, 1, 2, 255)) and SQLite cannot alter a CHECK
-- constraint in place, so code 3 is unstorable until the table is recreated.
-- Follows the established _v<N> rebuild used by 0012/0014/0017.
--
-- Clip targets carry no position by design: a clip is already a span, so a
-- position would permit a note claiming a clip while sitting outside that
-- clip's boundaries after a retime. Moment notes remain target_code 2.

CREATE TABLE pod0_notes_v33(
    note_id BLOB PRIMARY KEY NOT NULL CHECK(length(note_id) = 16),
    note_revision INTEGER NOT NULL CHECK(note_revision >= 1),
    text TEXT NOT NULL CHECK(length(CAST(text AS BLOB)) <= 65536),
    kind_code INTEGER NOT NULL CHECK(kind_code IN (1, 2, 3, 255)),
    kind_wire_code INTEGER,
    author_code INTEGER NOT NULL CHECK(author_code IN (1, 2, 255)),
    author_wire_code INTEGER,
    target_code INTEGER NOT NULL CHECK(target_code IN (0, 1, 2, 3, 255)),
    target_wire_code INTEGER,
    target_note_id BLOB CHECK(target_note_id IS NULL OR length(target_note_id) = 16),
    episode_id BLOB CHECK(episode_id IS NULL OR length(episode_id) = 16),
    position_ms INTEGER CHECK(position_ms IS NULL OR position_ms >= 0),
    target_clip_id BLOB REFERENCES pod0_clips(clip_id)
        CHECK(target_clip_id IS NULL OR length(target_clip_id) = 16),
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
    CHECK((target_code = 3) = (target_clip_id IS NOT NULL)),
    CHECK((source_import_id IS NULL) <> (created_command_id IS NULL)),
    CHECK(
        (evidence_generation_id IS NULL AND evidence_transcript_version_id IS NULL
            AND evidence_content_digest IS NULL AND evidence_span_id IS NULL)
        OR
        (evidence_generation_id IS NOT NULL AND evidence_transcript_version_id IS NOT NULL
            AND evidence_content_digest IS NOT NULL AND evidence_span_id IS NOT NULL)
    )
) STRICT;

INSERT INTO pod0_notes_v33(
    note_id,note_revision,text,kind_code,kind_wire_code,author_code,author_wire_code,
    target_code,target_wire_code,target_note_id,episode_id,position_ms,target_clip_id,
    created_at_ms,deleted,evidence_generation_id,evidence_transcript_version_id,
    evidence_content_digest,evidence_span_id,source_import_id,created_command_id
)
SELECT
    note_id,note_revision,text,kind_code,kind_wire_code,author_code,author_wire_code,
    target_code,target_wire_code,target_note_id,episode_id,position_ms,NULL,
    created_at_ms,deleted,evidence_generation_id,evidence_transcript_version_id,
    evidence_content_digest,evidence_span_id,source_import_id,created_command_id
FROM pod0_notes;

DROP TABLE pod0_notes;
ALTER TABLE pod0_notes_v33 RENAME TO pod0_notes;

CREATE INDEX pod0_notes_active_created_idx
ON pod0_notes(deleted, created_at_ms DESC, note_id);

CREATE INDEX pod0_notes_episode_position_idx
ON pod0_notes(episode_id, deleted, position_ms, note_id)
WHERE target_code = 2;

CREATE INDEX pod0_notes_clip_idx
ON pod0_notes(target_clip_id, deleted, created_at_ms DESC, note_id)
WHERE target_code = 3;
