use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use pod0_domain::TranscriptArtifact;

use crate::StorageError;
use crate::legacy_format::uuid_bytes;
use crate::legacy_transcript_db::{LegacyTranscriptRow, inspect_legacy_transcript_database};
use crate::legacy_transcript_format::RawTranscript;
use crate::legacy_transcript_transform::transform_transcript;
use crate::transcript_import_digest::{TranscriptImportHash, digest_bytes, parse_hex_digest};
use crate::transcript_import_model::{
    InspectedTranscriptEntry, InspectedTranscriptSource, TranscriptImportPlan,
};

const MAX_SELECTED_TRANSCRIPT_BYTES: u64 = 64 * 1_024 * 1_024;
const MAX_TOTAL_TRANSCRIPT_BYTES: u64 = 4 * 1_024 * 1_024 * 1_024;
const MAX_SELECTED_TRANSCRIPTS: usize = 50_000;

pub fn inspect_legacy_transcript_source(
    database_path: &Path,
    transcript_root: &Path,
) -> Result<TranscriptImportPlan, StorageError> {
    inspect_transcript_source(database_path, transcript_root).map(|source| source.plan)
}

pub(crate) fn inspect_transcript_source(
    database_path: &Path,
    transcript_root: &Path,
) -> Result<InspectedTranscriptSource, StorageError> {
    let database = inspect_legacy_transcript_database(database_path)?;
    if database.rows.len() > MAX_SELECTED_TRANSCRIPTS {
        return Err(StorageError::ImportLimitExceeded {
            entity: "selected transcripts",
        });
    }
    let mut total_bytes = 0_u64;
    let mut transcript_ids = BTreeSet::new();
    let mut entries = Vec::with_capacity(database.rows.len());
    for (offset, row) in database.rows.iter().enumerate() {
        let index = u32::try_from(offset).map_err(|_| StorageError::ImportLimitExceeded {
            entity: "selected transcripts",
        })?;
        let path = selected_path(row, transcript_root, index)?;
        let bytes = read_selected_file(&path, index)?;
        total_bytes = total_bytes.checked_add(bytes.len() as u64).ok_or(
            StorageError::ImportLimitExceeded {
                entity: "selected transcript bytes",
            },
        )?;
        if total_bytes > MAX_TOTAL_TRANSCRIPT_BYTES {
            return Err(StorageError::ImportLimitExceeded {
                entity: "selected transcript bytes",
            });
        }
        let file_digest = digest_bytes(&bytes);
        if parse_hex_digest(&row.content_hash)? != file_digest {
            return Err(invalid(
                index,
                "selected transcript content hash does not match file",
            ));
        }
        let raw: RawTranscript = serde_json::from_slice(&bytes)
            .map_err(|_| invalid(index, "selected transcript JSON is not recognized"))?;
        let transcript_id = uuid_bytes(&raw.id, "transcript", index)?;
        if !transcript_ids.insert(transcript_id) {
            return Err(invalid(index, "transcript identity is duplicated"));
        }
        let artifact =
            transform_transcript(raw, row.episode_id, row.podcast_id, file_digest, index)?;
        entries.push(InspectedTranscriptEntry {
            episode_id: row.episode_id,
            podcast_id: row.podcast_id,
            legacy_row_id: row.row_id,
            legacy_schema_version: row.artifact_schema_version,
            legacy_input_version: row.input_version.clone(),
            legacy_output_version: row.output_version.clone(),
            legacy_origin: row.origin.clone(),
            legacy_integrity: row.integrity.clone(),
            legacy_verified_at_ms: row.verified_at_ms,
            selected_row_digest: row.row_digest,
            selected_file_digest: file_digest,
            selected_file_byte_count: bytes.len() as u64,
            selected_file_path: path,
            artifact_id: artifact.artifact_id,
            transcript_version_id: artifact.transcript_version_id,
            transcript_content_digest: artifact.content_digest,
            artifact_integrity_digest: artifact.integrity_digest,
        });
    }
    let source_selection_digest = selection_digest(database.database_digest, &entries);
    Ok(InspectedTranscriptSource {
        plan: TranscriptImportPlan {
            source_kind: database.source_kind,
            source_generation: database.source_generation,
            source_database_digest: database.database_digest,
            source_selection_digest,
            selected_count: u32::try_from(entries.len()).map_err(|_| {
                StorageError::ImportLimitExceeded {
                    entity: "selected transcripts",
                }
            })?,
        },
        entries,
    })
}

pub(crate) fn selection_digest(
    database_digest: pod0_domain::ContentDigest,
    entries: &[InspectedTranscriptEntry],
) -> pod0_domain::ContentDigest {
    let mut hash = TranscriptImportHash::new(b"pod0.legacy-transcript-selection.v1");
    hash.bytes(&database_digest.into_bytes());
    hash.u64(entries.len() as u64);
    for entry in entries {
        hash.bytes(&entry.selected_row_digest.into_bytes());
        hash.bytes(&entry.selected_file_digest.into_bytes());
        hash.u64(entry.selected_file_byte_count);
        hash.bytes(&entry.artifact_id.into_bytes());
        hash.bytes(&entry.transcript_version_id.into_bytes());
    }
    hash.finish()
}

pub(crate) fn load_inspected_transcript_artifact(
    entry: &InspectedTranscriptEntry,
    index: u32,
) -> Result<TranscriptArtifact, StorageError> {
    let bytes = read_selected_file(&entry.selected_file_path, index)?;
    if bytes.len() as u64 != entry.selected_file_byte_count
        || digest_bytes(&bytes) != entry.selected_file_digest
    {
        return Err(invalid(
            index,
            "selected transcript changed after inspection",
        ));
    }
    let raw: RawTranscript = serde_json::from_slice(&bytes)
        .map_err(|_| invalid(index, "selected transcript JSON is not recognized"))?;
    let artifact = transform_transcript(
        raw,
        entry.episode_id,
        entry.podcast_id,
        entry.selected_file_digest,
        index,
    )?;
    if artifact.artifact_id != entry.artifact_id
        || artifact.transcript_version_id != entry.transcript_version_id
        || artifact.content_digest != entry.transcript_content_digest
        || artifact.integrity_digest != entry.artifact_integrity_digest
    {
        return Err(invalid(
            index,
            "selected transcript identity changed after inspection",
        ));
    }
    Ok(artifact)
}

fn selected_path(
    row: &LegacyTranscriptRow,
    root: &Path,
    index: u32,
) -> Result<PathBuf, StorageError> {
    match row.location.as_deref() {
        Some(value) => {
            let path = PathBuf::from(value);
            if !path.is_absolute() {
                return Err(invalid(
                    index,
                    "selected transcript location is not absolute",
                ));
            }
            Ok(path)
        }
        None => Ok(root.join(format!("{}.json", row.subject))),
    }
}

fn read_selected_file(path: &Path, index: u32) -> Result<Vec<u8>, StorageError> {
    let size = fs::metadata(path)
        .map_err(|error| StorageError::io("read selected transcript metadata", error))?
        .len();
    if size > MAX_SELECTED_TRANSCRIPT_BYTES {
        return Err(StorageError::ImportLimitExceeded {
            entity: "selected transcript file",
        });
    }
    let bytes =
        fs::read(path).map_err(|error| StorageError::io("read selected transcript file", error))?;
    if bytes.len() as u64 != size {
        return Err(invalid(index, "selected transcript changed while reading"));
    }
    Ok(bytes)
}

fn invalid(index: u32, detail: &'static str) -> StorageError {
    StorageError::InvalidLegacyRecord {
        entity: "transcript",
        index,
        detail,
    }
}
