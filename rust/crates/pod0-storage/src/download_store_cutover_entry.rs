use std::path::Path;

use rusqlite::{OptionalExtension, Transaction, params};

use crate::download_store_artifact_file::copy_and_hash_staged;
use crate::download_store_request::u64_to_i64;
use crate::{
    LegacyDownloadCutoverDisposition, LegacyDownloadCutoverEntry, LegacyDownloadCutoverInput,
    LibraryStore, StorageError,
};

struct PreparedArtifact {
    path: String,
    byte_count: u64,
    digest: [u8; 32],
}

pub(crate) struct PreparedEntry {
    entry: LegacyDownloadCutoverEntry,
    artifact: Option<PreparedArtifact>,
    pub(crate) repaired_invalid: bool,
}

pub(crate) fn prepare_entry(
    store: &LibraryStore,
    entry: LegacyDownloadCutoverEntry,
) -> Result<PreparedEntry, StorageError> {
    let artifact = match &entry.disposition {
        LegacyDownloadCutoverDisposition::Available {
            source_path,
            byte_count,
        } => match copy_and_hash_staged(
            store.path(),
            Path::new(source_path),
            entry.attempt_id,
            *byte_count,
        ) {
            Ok(staged) => Some(PreparedArtifact {
                path: staged
                    .pending_path
                    .to_str()
                    .ok_or(StorageError::InvalidDownloadArtifact)?
                    .to_owned(),
                byte_count: staged.byte_count,
                digest: staged.digest,
            }),
            Err(StorageError::InvalidDownloadArtifact) => None,
            Err(error) => return Err(error),
        },
        LegacyDownloadCutoverDisposition::Restart { .. } => None,
    };
    let repaired_invalid = matches!(
        entry.disposition,
        LegacyDownloadCutoverDisposition::Available { .. }
    ) && artifact.is_none();
    Ok(PreparedEntry {
        entry,
        artifact,
        repaired_invalid,
    })
}

pub(crate) fn insert_entry(
    transaction: &Transaction<'_>,
    input: &LegacyDownloadCutoverInput,
    prepared: &PreparedEntry,
) -> Result<(), StorageError> {
    let entry = &prepared.entry;
    let enclosure: Option<String> = transaction
        .query_row(
            "SELECT enclosure_url FROM pod0_episodes WHERE episode_id=?1",
            [entry.episode_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read legacy download episode", error))?;
    if enclosure.as_deref() != Some(entry.enclosure_url.as_str()) {
        return Err(StorageError::DownloadWorkflowConflict);
    }
    let (origin_code, origin_wire) = entry.origin.wire();
    let resume_key = match entry.disposition {
        LegacyDownloadCutoverDisposition::Restart {
            resume_available: true,
        } => Some(format!("v1/{}.resume", hex(&entry.attempt_id.into_bytes()))),
        _ => None,
    };
    let resume_key = resume_key.as_deref();
    let stage = if prepared.artifact.is_some() {
        "staged"
    } else {
        "requested"
    };
    transaction
        .execute(
            "INSERT INTO pod0_download_workflows(episode_id,intent_id,input_version,origin_code,\
             origin_wire_code,desired_state,stage,workflow_revision,attempt,attempt_id,request_id,\
             command_id,cancellation_id,issued_revision,deadline_at_ms,not_before_ms,enclosure_url,\
             resume_key,external_task_key,artifact_key,artifact_byte_count,artifact_digest,\
             failure_code,failure_detail,failure_retryable,created_at_ms,updated_at_ms) VALUES(\
             ?1,?2,?3,?4,?5,'present',?6,1,1,?7,?8,?9,?10,?11,?12,NULL,?13,?14,NULL,NULL,\
             NULL,NULL,NULL,NULL,0,?15,?15)",
            params![
                entry.episode_id.into_bytes().as_slice(),
                entry.intent_id.into_bytes().as_slice(),
                entry.input_version,
                origin_code,
                origin_wire,
                stage,
                entry.attempt_id.into_bytes().as_slice(),
                entry.request_id.into_bytes().as_slice(),
                entry.command_id.into_bytes().as_slice(),
                entry.cancellation_id.into_bytes().as_slice(),
                u64_to_i64(input.issued_revision.value)?,
                input.deadline_at_ms,
                entry.enclosure_url,
                resume_key,
                input.now_ms,
            ],
        )
        .map_err(|error| StorageError::sqlite("insert migrated download workflow", error))?;
    let artifact = prepared.artifact.as_ref();
    transaction
        .execute(
            "INSERT INTO pod0_download_attempts(attempt_id,episode_id,intent_id,attempt,state,\
             request_id,external_task_key,resume_key,staged_path,staged_byte_count,staged_digest,\
             failure_code,failure_detail,created_at_ms,updated_at_ms) VALUES(\
             ?1,?2,?3,1,?4,?5,NULL,?6,?7,?8,?9,NULL,NULL,?10,?10)",
            params![
                entry.attempt_id.into_bytes().as_slice(),
                entry.episode_id.into_bytes().as_slice(),
                entry.intent_id.into_bytes().as_slice(),
                stage,
                entry.request_id.into_bytes().as_slice(),
                resume_key,
                artifact.map(|value| value.path.as_str()),
                artifact
                    .map(|value| u64_to_i64(value.byte_count))
                    .transpose()?,
                artifact.map(|value| value.digest.as_slice()),
                input.now_ms,
            ],
        )
        .map_err(|error| StorageError::sqlite("insert migrated download attempt", error))?;
    transaction
        .execute(
            "INSERT INTO pod0_download_host_requests(request_id,episode_id,kind,state,command_id,\
             cancellation_id,issued_revision,deadline_at_ms,intent_id,attempt_id,input_version,\
             enclosure_url,resume_key,external_task_key,artifact_key,last_sequence_number,\
             created_at_ms,updated_at_ms) VALUES(?1,?2,'start','pending',?3,?4,?5,?6,?7,?8,\
             ?9,?10,?11,NULL,NULL,?12,?13,?13)",
            params![
                entry.request_id.into_bytes().as_slice(),
                entry.episode_id.into_bytes().as_slice(),
                entry.command_id.into_bytes().as_slice(),
                entry.cancellation_id.into_bytes().as_slice(),
                u64_to_i64(input.issued_revision.value)?,
                input.deadline_at_ms,
                entry.intent_id.into_bytes().as_slice(),
                entry.attempt_id.into_bytes().as_slice(),
                entry.input_version,
                entry.enclosure_url,
                resume_key,
                artifact.map(|_| 1_i64),
                input.now_ms,
            ],
        )
        .map_err(|error| StorageError::sqlite("insert migrated download request", error))?;
    Ok(())
}

fn hex(value: &[u8]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}
