use pod0_domain::{TranscriptArtifact, TranscriptArtifactId};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::StorageError;
use crate::listening_db_codec::transcript_source;
use crate::transcript_store_codec::artifact_id;
use crate::transcript_store_read_artifact::read_artifact_by_id;

pub(crate) fn transcript_is_authoritative(connection: &Connection) -> Result<bool, StorageError> {
    let state: Option<String> = connection
        .query_row(
            "SELECT state FROM pod0_domain_cutovers WHERE domain='transcripts'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read transcript authority", error))?;
    Ok(state.as_deref() == Some("authoritative"))
}

pub(crate) fn require_transcript_authoritative(
    connection: &Connection,
) -> Result<(), StorageError> {
    if transcript_is_authoritative(connection)? {
        Ok(())
    } else {
        Err(StorageError::CutoverNotAuthoritative)
    }
}

pub(crate) fn set_episode_transcript_available(
    transaction: &Transaction<'_>,
    artifact: &TranscriptArtifact,
) -> Result<(), StorageError> {
    let (source_code, source_wire_code) = transcript_source(&artifact.provenance.source);
    let reference = artifact_reference(artifact.artifact_id);
    let changed = transaction
        .execute(
            "UPDATE pod0_episodes SET transcript_code=2,transcript_wire_code=NULL,\
             transcript_ref_version=?1,transcript_ref_key=?2,transcript_source_code=?3,\
             transcript_source_wire_code=?4 WHERE episode_id=?5",
            params![
                i64::from(artifact.schema_version),
                reference,
                source_code,
                source_wire_code,
                artifact.episode_id.into_bytes().as_slice(),
            ],
        )
        .map_err(|error| StorageError::sqlite("project selected transcript readiness", error))?;
    if changed == 1 {
        Ok(())
    } else {
        Err(StorageError::InvalidTranscriptArtifact)
    }
}

pub(crate) fn synchronize_episode_transcript_readiness(
    transaction: &Transaction<'_>,
) -> Result<(), StorageError> {
    transaction
        .execute(
            "UPDATE pod0_episodes SET transcript_code=1,transcript_wire_code=NULL,\
             transcript_ref_version=NULL,transcript_ref_key=NULL,transcript_source_code=NULL,\
             transcript_source_wire_code=NULL",
            [],
        )
        .map_err(|error| StorageError::sqlite("clear legacy transcript readiness", error))?;
    let artifact_ids = {
        let mut statement = transaction
            .prepare("SELECT artifact_id FROM pod0_transcript_selection ORDER BY episode_id")
            .map_err(|error| StorageError::sqlite("prepare transcript readiness", error))?;
        let rows = statement
            .query_map([], |row| row.get::<_, Vec<u8>>(0))
            .map_err(|error| StorageError::sqlite("read transcript readiness", error))?;
        rows.map(|row| {
            let bytes =
                row.map_err(|error| StorageError::sqlite("decode transcript readiness", error))?;
            artifact_id(&bytes)
        })
        .collect::<Result<Vec<_>, StorageError>>()?
    };
    for selected_id in artifact_ids {
        let artifact = read_artifact_by_id(transaction, selected_id)?
            .ok_or(StorageError::InvalidTranscriptArtifact)?;
        set_episode_transcript_available(transaction, &artifact)?;
    }
    Ok(())
}

pub(crate) fn advance_listening_revision(
    transaction: &Transaction<'_>,
) -> Result<u64, StorageError> {
    let listening_state: Option<String> = transaction
        .query_row(
            "SELECT state FROM pod0_domain_cutovers WHERE domain='listening'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read listening authority", error))?;
    if listening_state.as_deref() != Some("authoritative") {
        return Err(StorageError::CutoverNotAuthoritative);
    }
    let current: i64 = transaction
        .query_row(
            "SELECT state_revision FROM pod0_playback_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read listening revision", error))?;
    let next = current.checked_add(1).ok_or(StorageError::CorruptSchema {
        detail: "listening revision exhausted during transcript update",
    })?;
    transaction
        .execute(
            "UPDATE pod0_playback_state SET state_revision=?1 WHERE singleton=1",
            [next],
        )
        .map_err(|error| StorageError::sqlite("advance transcript listening revision", error))?;
    transaction
        .execute(
            "UPDATE pod0_domain_cutovers SET core_revision=?1 WHERE domain='listening'",
            [next],
        )
        .map_err(|error| StorageError::sqlite("advance listening cutover revision", error))?;
    u64::try_from(next).map_err(|_| StorageError::CorruptSchema {
        detail: "listening revision is malformed",
    })
}

pub(crate) fn set_transcript_cutover_revision(
    transaction: &Transaction<'_>,
    revision: u64,
) -> Result<(), StorageError> {
    let value = i64::try_from(revision).map_err(|_| StorageError::TranscriptRevisionConflict)?;
    let changed = transaction
        .execute(
            "UPDATE pod0_domain_cutovers SET core_revision=?1 WHERE domain='transcripts' \
             AND state='authoritative'",
            [value],
        )
        .map_err(|error| StorageError::sqlite("advance transcript cutover revision", error))?;
    if changed == 1 {
        Ok(())
    } else {
        Err(StorageError::CutoverNotAuthoritative)
    }
}

fn artifact_reference(artifact_id: TranscriptArtifactId) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let bytes = artifact_id.into_bytes();
    let mut encoded = String::with_capacity(32);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    format!("pod0-transcript:{encoded}:v1")
}
