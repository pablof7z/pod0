use pod0_domain::{CommandId, TranscriptEvidenceArtifact};
use rusqlite::{OptionalExtension, Transaction, params};

use crate::evidence_codec::{encode_source, podcast_id, sqlite_i64};
use crate::evidence_commands::{EvidenceOperation, fingerprint, record, replay};
use crate::evidence_store::EvidenceStore;
use crate::evidence_store_read::read_artifact;
use crate::{EvidenceStageReceipt, StorageError};

impl EvidenceStore {
    pub fn stage_artifact(
        &self,
        command_id: CommandId,
        artifact: &TranscriptEvidenceArtifact,
        staged_at_ms: i64,
    ) -> Result<EvidenceStageReceipt, StorageError> {
        self.stage_artifact_with_observer(command_id, artifact, staged_at_ms, || Ok(()))
    }

    pub(crate) fn stage_artifact_with_observer<F>(
        &self,
        command_id: CommandId,
        artifact: &TranscriptEvidenceArtifact,
        staged_at_ms: i64,
        before_commit: F,
    ) -> Result<EvidenceStageReceipt, StorageError>
    where
        F: FnOnce() -> Result<(), StorageError>,
    {
        artifact
            .verify_integrity()
            .map_err(crate::evidence_codec::artifact_error)?;
        validate_sqlite_values(artifact)?;
        let generation_id = artifact.generation_id;
        let command_fingerprint = fingerprint(
            EvidenceOperation::Stage,
            generation_id,
            None,
            Some(artifact.integrity_digest),
        );
        self.write(|transaction| {
            if let Some(stored) = replay(transaction, command_id, command_fingerprint)? {
                if stored.operation != EvidenceOperation::Stage
                    || stored.generation_id != generation_id
                    || stored.episode_id.is_some()
                {
                    return Err(StorageError::EvidenceCommandConflict);
                }
                return Ok(EvidenceStageReceipt {
                    generation_id,
                    already_present: stored.result,
                });
            }
            require_episode_parent(transaction, artifact)?;
            let already_present = match read_artifact(transaction, generation_id)? {
                Some(stored) if stored == *artifact => true,
                Some(_) => return Err(StorageError::InvalidEvidenceArtifact),
                None => {
                    insert_artifact(transaction, artifact, staged_at_ms)?;
                    let stored = read_artifact(transaction, generation_id)?
                        .ok_or(StorageError::InvalidEvidenceArtifact)?;
                    if stored != *artifact {
                        return Err(StorageError::InvalidEvidenceArtifact);
                    }
                    false
                }
            };
            record(
                transaction,
                command_id,
                EvidenceOperation::Stage,
                command_fingerprint,
                generation_id,
                None,
                None,
                already_present,
                staged_at_ms,
            )?;
            before_commit()?;
            Ok(EvidenceStageReceipt {
                generation_id,
                already_present,
            })
        })
    }
}

fn require_episode_parent(
    transaction: &Transaction<'_>,
    artifact: &TranscriptEvidenceArtifact,
) -> Result<(), StorageError> {
    let stored = transaction
        .query_row(
            "SELECT podcast_id FROM pod0_episodes WHERE episode_id=?1",
            [artifact.version.episode_id.into_bytes().as_slice()],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read evidence episode parent", error))?;
    let Some(stored) = stored else {
        return Err(StorageError::EvidenceEpisodeMismatch);
    };
    if podcast_id(&stored)? == artifact.version.podcast_id {
        Ok(())
    } else {
        Err(StorageError::EvidenceEpisodeMismatch)
    }
}

fn insert_artifact(
    transaction: &Transaction<'_>,
    artifact: &TranscriptEvidenceArtifact,
    staged_at_ms: i64,
) -> Result<(), StorageError> {
    insert_document(transaction, artifact)?;
    insert_segments(transaction, artifact)?;
    transaction
        .execute(
            "INSERT OR IGNORE INTO pod0_evidence_generations(generation_id,transcript_version_id,\
             episode_id,artifact_schema_version,integrity_digest,chunk_policy_version,target_tokens,\
             overlap_per_mille,snap_tolerance_per_mille,span_count,state,staged_at_ms) \
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'staged',?11)",
            params![
                artifact.generation_id.into_bytes().as_slice(),
                artifact.version.transcript_version_id.into_bytes().as_slice(),
                artifact.version.episode_id.into_bytes().as_slice(),
                artifact.schema_version,
                artifact.integrity_digest.into_bytes().as_slice(),
                artifact.policy.version,
                artifact.policy.target_tokens,
                artifact.policy.overlap_per_mille,
                artifact.policy.snap_tolerance_per_mille,
                i64::try_from(artifact.spans.len())
                    .map_err(|_| StorageError::InvalidEvidenceArtifact)?,
                staged_at_ms,
            ],
        )
        .map_err(|error| StorageError::sqlite("insert evidence generation", error))?;
    insert_spans(transaction, artifact)
}

fn insert_document(
    transaction: &Transaction<'_>,
    artifact: &TranscriptEvidenceArtifact,
) -> Result<(), StorageError> {
    let version = &artifact.version;
    let (source_code, source_wire) = encode_source(&version.provenance.source);
    transaction
        .execute(
            "INSERT OR IGNORE INTO pod0_transcript_documents(transcript_version_id,episode_id,\
             podcast_id,source_revision,content_digest,source_code,source_wire_code,provider,\
             source_payload_digest,segment_count) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![
                version.transcript_version_id.into_bytes().as_slice(),
                version.episode_id.into_bytes().as_slice(),
                version.podcast_id.into_bytes().as_slice(),
                version.source_revision,
                version.content_digest.into_bytes().as_slice(),
                source_code,
                source_wire,
                version.provenance.provider,
                version
                    .provenance
                    .source_payload_digest
                    .into_bytes()
                    .as_slice(),
                i64::try_from(artifact.segments.len())
                    .map_err(|_| StorageError::InvalidEvidenceArtifact)?,
            ],
        )
        .map_err(|error| StorageError::sqlite("insert transcript document", error))?;
    Ok(())
}

fn insert_segments(
    transaction: &Transaction<'_>,
    artifact: &TranscriptEvidenceArtifact,
) -> Result<(), StorageError> {
    let version_id = artifact.version.transcript_version_id.into_bytes();
    let mut statement = transaction
        .prepare(
            "INSERT OR IGNORE INTO pod0_transcript_segments(segment_id,\
                         transcript_version_id,ordinal,text,start_ms,end_ms,speaker_id) \
                         VALUES(?1,?2,?3,?4,?5,?6,?7)",
        )
        .map_err(|error| StorageError::sqlite("prepare transcript segment insert", error))?;
    for segment in &artifact.segments {
        statement
            .execute(params![
                segment.segment_id.into_bytes().as_slice(),
                version_id.as_slice(),
                segment.ordinal,
                segment.text,
                sqlite_i64(segment.start_milliseconds)?,
                sqlite_i64(segment.end_milliseconds)?,
                segment.speaker_id.map(|id| id.into_bytes().to_vec()),
            ])
            .map_err(|error| StorageError::sqlite("insert transcript segment", error))?;
    }
    Ok(())
}

fn insert_spans(
    transaction: &Transaction<'_>,
    artifact: &TranscriptEvidenceArtifact,
) -> Result<(), StorageError> {
    let generation_id = artifact.generation_id.into_bytes();
    let mut statement = transaction
        .prepare(
            "INSERT OR IGNORE INTO pod0_evidence_spans(span_id,generation_id,\
                         sort_order,first_segment_id,last_segment_id,start_segment_ordinal,\
                         end_segment_ordinal_exclusive,start_ms,end_ms,text,speaker_id,\
                         chunk_policy_version) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
        )
        .map_err(|error| StorageError::sqlite("prepare evidence span insert", error))?;
    for (index, span) in artifact.spans.iter().enumerate() {
        statement
            .execute(params![
                span.span_id.into_bytes().as_slice(),
                generation_id.as_slice(),
                i64::try_from(index).map_err(|_| StorageError::InvalidEvidenceArtifact)?,
                span.first_segment_id.into_bytes().as_slice(),
                span.last_segment_id.into_bytes().as_slice(),
                span.start_segment_ordinal,
                span.end_segment_ordinal_exclusive,
                sqlite_i64(span.start_milliseconds)?,
                sqlite_i64(span.end_milliseconds)?,
                span.text,
                span.speaker_id.map(|id| id.into_bytes().to_vec()),
                span.chunk_policy_version,
            ])
            .map_err(|error| StorageError::sqlite("insert evidence span", error))?;
    }
    Ok(())
}

fn validate_sqlite_values(artifact: &TranscriptEvidenceArtifact) -> Result<(), StorageError> {
    for segment in &artifact.segments {
        sqlite_i64(segment.start_milliseconds)?;
        sqlite_i64(segment.end_milliseconds)?;
    }
    for span in &artifact.spans {
        sqlite_i64(span.start_milliseconds)?;
        sqlite_i64(span.end_milliseconds)?;
    }
    Ok(())
}
