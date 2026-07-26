use std::path::Path;

use rusqlite::Connection;

/// Opens `path` and reverts its notes table. See [`revert_notes_below_v33_on`].
pub(crate) fn revert_notes_below_v33(path: &Path) {
    revert_notes_below_v33_on(&Connection::open(path).unwrap());
}

/// Rewrites `pod0_notes` back to its pre-33 shape.
///
/// These fixtures fake an older store by migrating to `CURRENT_SCHEMA_VERSION`
/// and then hand-reverting only the chapter tables before stamping the version
/// down. That shortcut quietly assumed no migration would ever change a table
/// outside the chapter family — true until schema 33 added
/// `pod0_notes.target_clip_id`, at which point the fixture claimed to be a v13
/// store while carrying a v33 notes table, and `validate_open_database`
/// correctly rejected it.
///
/// `ALTER TABLE ... DROP COLUMN` cannot be used here: `target_clip_id` appears
/// in a CHECK constraint, which SQLite refuses to drop a column out of. So the
/// pre-33 table is rebuilt the same way the real migration builds the new one.
pub(crate) fn revert_notes_below_v33_on(connection: &Connection) {
    connection
        .execute_batch(
            "CREATE TABLE pod0_notes_pre33(
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
                 CHECK(evidence_transcript_version_id IS NULL
                   OR length(evidence_transcript_version_id) = 16),
               evidence_content_digest BLOB
                 CHECK(evidence_content_digest IS NULL OR length(evidence_content_digest) = 32),
               evidence_span_id BLOB
                 CHECK(evidence_span_id IS NULL OR length(evidence_span_id) = 16),
               source_import_id BLOB REFERENCES pod0_note_imports(import_id),
               created_command_id BLOB
                 CHECK(created_command_id IS NULL OR length(created_command_id) = 16),
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
                 (evidence_generation_id IS NOT NULL
                   AND evidence_transcript_version_id IS NOT NULL
                   AND evidence_content_digest IS NOT NULL AND evidence_span_id IS NOT NULL)
               )
             ) STRICT;
             INSERT INTO pod0_notes_pre33(
               note_id,note_revision,text,kind_code,kind_wire_code,author_code,author_wire_code,
               target_code,target_wire_code,target_note_id,episode_id,position_ms,
               created_at_ms,deleted,evidence_generation_id,evidence_transcript_version_id,
               evidence_content_digest,evidence_span_id,source_import_id,created_command_id
             )
             SELECT
               note_id,note_revision,text,kind_code,kind_wire_code,author_code,author_wire_code,
               target_code,target_wire_code,target_note_id,episode_id,position_ms,
               created_at_ms,deleted,evidence_generation_id,evidence_transcript_version_id,
               evidence_content_digest,evidence_span_id,source_import_id,created_command_id
             FROM pod0_notes WHERE target_code <> 3;
             DROP TABLE pod0_notes;
             ALTER TABLE pod0_notes_pre33 RENAME TO pod0_notes;
             CREATE INDEX pod0_notes_active_created_idx
               ON pod0_notes(deleted, created_at_ms DESC, note_id);
             CREATE INDEX pod0_notes_episode_position_idx
               ON pod0_notes(episode_id, deleted, position_ms, note_id)
               WHERE target_code = 2;",
        )
        .unwrap();
}
