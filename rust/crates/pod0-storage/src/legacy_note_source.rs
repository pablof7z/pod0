use std::collections::BTreeSet;
use std::fs::File;
use std::io::{Read, Seek};
use std::path::Path;

use pod0_domain::{
    EpisodeId, MAX_NOTE_TEXT_BYTES, NoteAuthor, NoteId, NoteKind, NoteRecord, NoteRevision,
    NoteTarget, UnixTimestampMilliseconds,
};
use rusqlite::{Connection, OpenFlags};
use serde_json::from_slice;
use sha2::{Digest, Sha256};

use crate::backup::verify_connection;
use crate::legacy_format::{RawAppState, finite_milliseconds, timestamp_milliseconds, uuid_bytes};
use crate::note_import_model::{InspectedNoteSource, NoteImportPlan};
use crate::{LegacySourceKind, StorageError};

const SQLITE_HEADER: &[u8; 16] = b"SQLite format 3\0";
const MAX_SOURCE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_NOTES: usize = 100_000;

pub fn inspect_legacy_note_source(path: &Path) -> Result<NoteImportPlan, StorageError> {
    inspect_note_source(path).map(|source| source.plan)
}

pub(crate) fn inspect_note_source(path: &Path) -> Result<InspectedNoteSource, StorageError> {
    let mut file = File::open(path).map_err(|error| StorageError::io("open note source", error))?;
    let mut header = [0_u8; 16];
    let read = file
        .read(&mut header)
        .map_err(|error| StorageError::io("read note source header", error))?;
    file.rewind()
        .map_err(|error| StorageError::io("rewind note source", error))?;
    let (kind, generation, raw) = if read == SQLITE_HEADER.len() && &header == SQLITE_HEADER {
        load_sqlite(path)?
    } else {
        load_json(file)?
    };
    transform(kind, generation, raw)
}

fn load_json(mut file: File) -> Result<(LegacySourceKind, u64, RawAppState), StorageError> {
    let size = file
        .metadata()
        .map_err(|error| StorageError::io("read note JSON metadata", error))?
        .len();
    if size > MAX_SOURCE_BYTES {
        return Err(StorageError::ImportLimitExceeded {
            entity: "legacy note JSON bytes",
        });
    }
    let mut bytes = Vec::with_capacity(usize::try_from(size).unwrap_or(0));
    file.read_to_end(&mut bytes)
        .map_err(|error| StorageError::io("read note JSON", error))?;
    let raw: RawAppState = from_slice(&bytes).map_err(|_| StorageError::InvalidLegacyRecord {
        entity: "notes metadata",
        index: 0,
        detail: "legacy notes are not recognized JSON",
    })?;
    Ok((LegacySourceKind::LegacyJson, raw.generation, raw))
}

fn load_sqlite(path: &Path) -> Result<(LegacySourceKind, u64, RawAppState), StorageError> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_FULL_MUTEX,
    )
    .map_err(|error| StorageError::sqlite("open Swift note source", error))?;
    connection
        .execute_batch("PRAGMA query_only=ON; PRAGMA foreign_keys=ON;")
        .map_err(|error| StorageError::sqlite("configure Swift note source", error))?;
    verify_connection(&connection)?;
    let metadata: Vec<u8> = connection
        .query_row(
            "SELECT value FROM persistence_metadata WHERE key='app_state'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read Swift note metadata", error))?;
    if metadata.len() as u64 > MAX_SOURCE_BYTES {
        return Err(StorageError::ImportLimitExceeded {
            entity: "legacy note metadata bytes",
        });
    }
    let generation_text: String = connection
        .query_row(
            "SELECT CAST(value AS TEXT) FROM persistence_metadata WHERE key='generation'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read Swift note generation", error))?;
    let generation =
        generation_text
            .parse::<u64>()
            .map_err(|_| StorageError::InvalidLegacyRecord {
                entity: "notes metadata",
                index: 0,
                detail: "note source generation is invalid",
            })?;
    let mut raw: RawAppState =
        from_slice(&metadata).map_err(|_| StorageError::InvalidLegacyRecord {
            entity: "notes metadata",
            index: 0,
            detail: "Swift notes metadata is not recognized JSON",
        })?;
    if raw.generation != 0 && raw.generation != generation {
        return Err(StorageError::InvalidLegacyRecord {
            entity: "notes metadata",
            index: 0,
            detail: "note metadata and SQLite generations differ",
        });
    }
    raw.generation = generation;
    Ok((LegacySourceKind::SwiftSqlite, generation, raw))
}

fn transform(
    source_kind: LegacySourceKind,
    source_generation: u64,
    raw: RawAppState,
) -> Result<InspectedNoteSource, StorageError> {
    if raw.notes.len() > MAX_NOTES {
        return Err(StorageError::ImportLimitExceeded { entity: "notes" });
    }
    let mut identities = BTreeSet::new();
    let mut notes = Vec::with_capacity(raw.notes.len());
    for (offset, raw) in raw.notes.into_iter().enumerate() {
        let index = u32::try_from(offset)
            .map_err(|_| StorageError::ImportLimitExceeded { entity: "notes" })?;
        let note_id = NoteId::from_bytes(uuid_bytes(&raw.id, "note", index)?);
        if !identities.insert(note_id) {
            return Err(StorageError::InvalidLegacyRecord {
                entity: "note",
                index,
                detail: "note identity is duplicated",
            });
        }
        if raw.text.len() > MAX_NOTE_TEXT_BYTES {
            return Err(StorageError::ImportLimitExceeded {
                entity: "note text",
            });
        }
        let kind = match raw.kind.as_deref().unwrap_or("free") {
            "free" => NoteKind::Free,
            "reflection" => NoteKind::Reflection,
            "systemEvent" => NoteKind::SystemEvent,
            _ => return Err(invalid(index, "note kind is unsupported")),
        };
        let author = match raw.author.as_deref().unwrap_or("user") {
            "user" => NoteAuthor::User,
            "agent" => NoteAuthor::Agent,
            _ => return Err(invalid(index, "note author is unsupported")),
        };
        let target = raw
            .target
            .map(|target| match target.kind.as_str() {
                "note" => Ok(NoteTarget::Note {
                    note_id: NoteId::from_bytes(uuid_bytes(&target.id, "note target", index)?),
                }),
                "episode" => Ok(NoteTarget::Episode {
                    episode_id: EpisodeId::from_bytes(uuid_bytes(
                        &target.id,
                        "note episode target",
                        index,
                    )?),
                    position_milliseconds: u64::try_from(finite_milliseconds(
                        target.position_seconds.unwrap_or(0.0),
                        "note position",
                        index,
                    )?)
                    .map_err(|_| invalid(index, "note position is outside supported range"))?,
                }),
                _ => Err(invalid(index, "note target is unsupported")),
            })
            .transpose()?;
        let created_at = raw
            .created_at
            .as_ref()
            .ok_or_else(|| invalid(index, "note creation time is missing"))?;
        notes.push(NoteRecord {
            note_id,
            revision: NoteRevision::INITIAL,
            text: raw.text,
            kind,
            author,
            target,
            created_at: UnixTimestampMilliseconds::new(timestamp_milliseconds(
                Some(created_at),
                "note",
                index,
            )?),
            deleted: raw.deleted,
            evidence: None,
        });
    }
    notes.sort_by(|left, right| {
        right
            .created_at
            .value
            .cmp(&left.created_at.value)
            .then_with(|| left.note_id.cmp(&right.note_id))
    });
    let source_hash = digest(&notes);
    Ok(InspectedNoteSource {
        plan: NoteImportPlan {
            source_kind,
            source_hash,
            source_generation,
            note_count: u32::try_from(notes.len())
                .map_err(|_| StorageError::ImportLimitExceeded { entity: "notes" })?,
        },
        notes,
    })
}

fn digest(notes: &[NoteRecord]) -> String {
    let mut hash = Sha256::new();
    part(&mut hash, b"pod0-legacy-notes-v1");
    for note in notes {
        part(&mut hash, &note.note_id.into_bytes());
        part(&mut hash, note.text.as_bytes());
        hash.update([match note.kind {
            NoteKind::Free => 1,
            NoteKind::Reflection => 2,
            NoteKind::SystemEvent => 3,
            NoteKind::Unsupported { .. } => 255,
        }]);
        hash.update([match note.author {
            NoteAuthor::User => 1,
            NoteAuthor::Agent => 2,
            NoteAuthor::Unsupported { .. } => 255,
        }]);
        match note.target {
            None => hash.update([0]),
            Some(NoteTarget::Note { note_id }) => {
                hash.update([1]);
                hash.update(note_id.into_bytes());
            }
            Some(NoteTarget::Episode {
                episode_id,
                position_milliseconds,
            }) => {
                hash.update([2]);
                hash.update(episode_id.into_bytes());
                hash.update(position_milliseconds.to_be_bytes());
            }
            Some(NoteTarget::Unsupported { wire_code }) => {
                hash.update([255]);
                hash.update(wire_code.to_be_bytes());
            }
        }
        hash.update(note.created_at.value.to_be_bytes());
        hash.update([u8::from(note.deleted)]);
    }
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn part(hash: &mut Sha256, value: &[u8]) {
    hash.update(u64::try_from(value.len()).unwrap_or(u64::MAX).to_be_bytes());
    hash.update(value);
}

fn invalid(index: u32, detail: &'static str) -> StorageError {
    StorageError::InvalidLegacyRecord {
        entity: "note",
        index,
        detail,
    }
}
