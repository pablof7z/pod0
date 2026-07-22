use pod0_domain::{
    CancellationId, CommandId, ContentDigest, EpisodeId, HostRequestId, StateRevision,
    TranscriptArtifactId, TranscriptVersionId,
};
use rusqlite::params;

use super::authority::read_authority;
use super::model::{PreparedTranscriptAttempt, StoredTranscriptWorkflowRequest};
use super::support::i64_value;
use crate::{LibraryStore, StorageError};

pub const MAX_LEGACY_TRANSCRIPT_WORKFLOW_ROWS: usize = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TranscriptWorkflowAuthorityState {
    NotStarted,
    Staged { source_generation: u64 },
    Verified { source_generation: u64 },
    Authoritative { source_generation: u64 },
}

impl TranscriptWorkflowAuthorityState {
    pub const fn is_authoritative(self) -> bool {
        matches!(self, Self::Authoritative { .. })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacyTranscriptWorkflowRowClassification {
    Restart,
    RecoverProvider,
    Ambiguous,
    Blocked,
    Failed,
    Cancelled,
    Succeeded,
    IndexPending,
    IndexSucceeded,
    Obsolete,
}

impl LegacyTranscriptWorkflowRowClassification {
    pub(crate) const fn wire(self) -> &'static str {
        match self {
            Self::Restart => "restart",
            Self::RecoverProvider => "recover_provider",
            Self::Ambiguous => "ambiguous",
            Self::Blocked => "blocked",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Succeeded => "succeeded",
            Self::IndexPending => "index_pending",
            Self::IndexSucceeded => "index_succeeded",
            Self::Obsolete => "obsolete",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "restart" => Self::Restart,
            "recover_provider" => Self::RecoverProvider,
            "ambiguous" => Self::Ambiguous,
            "blocked" => Self::Blocked,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            "succeeded" => Self::Succeeded,
            "index_pending" => Self::IndexPending,
            "index_succeeded" => Self::IndexSucceeded,
            "obsolete" => Self::Obsolete,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyTranscriptWorkflowBackupRow {
    pub episode_id: EpisodeId,
    pub row_bytes: Vec<u8>,
    pub row_fingerprint: ContentDigest,
    pub classification: LegacyTranscriptWorkflowRowClassification,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LegacyTranscriptWorkflowDisposition {
    Restart,
    RecoverProvider {
        external_operation_id: String,
        provider_status: Option<String>,
    },
    Ambiguous,
    Blocked {
        failure_code: String,
        failure_detail: Option<String>,
        may_have_submitted: bool,
    },
    Failed {
        failure_code: String,
        failure_detail: Option<String>,
        may_have_submitted: bool,
    },
    Cancelled {
        may_have_submitted: bool,
    },
    Succeeded {
        artifact_id: TranscriptArtifactId,
        transcript_version_id: TranscriptVersionId,
        content_digest: ContentDigest,
        selection_revision: StateRevision,
    },
    IndexPending {
        artifact_id: TranscriptArtifactId,
        transcript_version_id: TranscriptVersionId,
        content_digest: ContentDigest,
        selection_revision: StateRevision,
        evidence_input_version: String,
    },
    IndexSucceeded {
        artifact_id: TranscriptArtifactId,
        transcript_version_id: TranscriptVersionId,
        content_digest: ContentDigest,
        selection_revision: StateRevision,
        evidence_input_version: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyTranscriptWorkflowCandidate {
    pub episode_id: EpisodeId,
    pub request: StoredTranscriptWorkflowRequest,
    pub request_id: Option<HostRequestId>,
    pub prepared_attempt: Option<PreparedTranscriptAttempt>,
    pub deadline_at_ms: Option<i64>,
    pub expected_selection_revision: StateRevision,
    pub disposition: LegacyTranscriptWorkflowDisposition,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyTranscriptWorkflowCutoverInput {
    pub source_generation: u64,
    pub source_fingerprint: ContentDigest,
    pub backup_digest: ContentDigest,
    pub backup_byte_count: u64,
    pub rows: Vec<LegacyTranscriptWorkflowBackupRow>,
    pub candidates: Vec<LegacyTranscriptWorkflowCandidate>,
    pub command_id: CommandId,
    pub cancellation_id: CancellationId,
    pub issued_revision: StateRevision,
    pub max_attempts: u16,
    pub now_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyTranscriptWorkflowCutoverReport {
    pub state: TranscriptWorkflowAuthorityState,
    pub source_fingerprint: ContentDigest,
    pub row_count: u32,
    pub adopted_workflow_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptWorkflowRollbackExport {
    pub source_generation: u64,
    pub source_fingerprint: ContentDigest,
    pub backup_digest: ContentDigest,
    pub backup_byte_count: u64,
    pub rows: Vec<LegacyTranscriptWorkflowBackupRow>,
}

impl LibraryStore {
    pub fn transcript_workflow_cutover_report(
        &self,
    ) -> Result<Option<LegacyTranscriptWorkflowCutoverReport>, StorageError> {
        self.read(|connection| {
            let source_generation = match read_authority(connection)? {
                TranscriptWorkflowAuthorityState::NotStarted => return Ok(None),
                TranscriptWorkflowAuthorityState::Staged { source_generation }
                | TranscriptWorkflowAuthorityState::Verified { source_generation }
                | TranscriptWorkflowAuthorityState::Authoritative { source_generation } => {
                    source_generation
                }
            };
            super::cutover_stage::cutover_report(connection, source_generation).map(Some)
        })
    }

    pub fn transcript_workflow_authority(
        &self,
    ) -> Result<TranscriptWorkflowAuthorityState, StorageError> {
        self.read(read_authority)
    }

    pub fn verify_legacy_transcript_workflow_cutover(
        &self,
        source_generation: u64,
        source_fingerprint: ContentDigest,
        verified_at_ms: i64,
    ) -> Result<LegacyTranscriptWorkflowCutoverReport, StorageError> {
        if source_generation == 0 || verified_at_ms < 0 {
            return Err(StorageError::TranscriptWorkflowConflict);
        }
        self.write(|transaction| {
            super::cutover_stage::verify_stored_import(
                transaction,
                source_generation,
                source_fingerprint,
            )?;
            transaction
                .execute(
                    "UPDATE pod0_transcript_workflow_imports SET state='verified',verified_at_ms=?1
                 WHERE singleton=1 AND source_generation=?2 AND source_fingerprint=?3
                 AND state IN('staged','verified')",
                    params![
                        verified_at_ms,
                        i64_value(source_generation)?,
                        source_fingerprint.into_bytes().as_slice()
                    ],
                )
                .map_err(|error| {
                    StorageError::sqlite("verify transcript workflow cutover", error)
                })?;
            super::cutover_stage::cutover_report(transaction, source_generation)
        })
    }

    pub fn commit_legacy_transcript_workflow_cutover(
        &self,
        source_generation: u64,
        verified_source_fingerprint: ContentDigest,
        committed_at_ms: i64,
    ) -> Result<TranscriptWorkflowAuthorityState, StorageError> {
        self.commit_legacy_transcript_workflow_cutover_with_observer(
            source_generation,
            verified_source_fingerprint,
            committed_at_ms,
            || Ok(()),
        )
    }

    pub(crate) fn commit_legacy_transcript_workflow_cutover_with_observer<F>(
        &self,
        source_generation: u64,
        verified_source_fingerprint: ContentDigest,
        committed_at_ms: i64,
        before_commit: F,
    ) -> Result<TranscriptWorkflowAuthorityState, StorageError>
    where
        F: FnOnce() -> Result<(), StorageError>,
    {
        if source_generation == 0 || committed_at_ms < 0 {
            return Err(StorageError::TranscriptWorkflowConflict);
        }
        self.write(|transaction| {
            match read_authority(transaction)? {
                TranscriptWorkflowAuthorityState::Authoritative { source_generation: current }
                    if current == source_generation => return Ok(TranscriptWorkflowAuthorityState::Authoritative { source_generation }),
                TranscriptWorkflowAuthorityState::Verified { source_generation: current }
                    if current == source_generation => {}
                _ => return Err(StorageError::TranscriptWorkflowConflict),
            }
            super::cutover_stage::verify_stored_import(transaction,source_generation,verified_source_fingerprint)?;
            let changed = transaction.execute(
                "UPDATE pod0_transcript_workflow_imports SET state='authoritative',committed_at_ms=?1
                 WHERE singleton=1 AND source_generation=?2 AND source_fingerprint=?3 AND state='verified'",
                params![committed_at_ms,i64_value(source_generation)?,verified_source_fingerprint.into_bytes().as_slice()],
            ).map_err(|error| StorageError::sqlite("commit transcript workflow import", error))?;
            if changed != 1 { return Err(StorageError::TranscriptWorkflowConflict); }
            let changed = transaction.execute(
                "UPDATE pod0_domain_cutovers SET state='authoritative',committed_at_ms=?1
                 WHERE domain='transcript_workflows' AND source_generation=?2 AND state='staged'",
                params![committed_at_ms,i64_value(source_generation)?],
            ).map_err(|error| StorageError::sqlite("commit transcript workflow authority", error))?;
            if changed != 1 { return Err(StorageError::TranscriptWorkflowConflict); }
            before_commit()?;
            Ok(TranscriptWorkflowAuthorityState::Authoritative { source_generation })
        })
    }

    pub fn discard_legacy_transcript_workflow_cutover(
        &self,
        source_generation: u64,
    ) -> Result<bool, StorageError> {
        self.write(|transaction| match read_authority(transaction)? {
            TranscriptWorkflowAuthorityState::Staged { source_generation: current }
            | TranscriptWorkflowAuthorityState::Verified { source_generation: current }
                if current == source_generation => {
                    transaction.execute("DELETE FROM pod0_transcript_workflows WHERE source_generation=?1",
                        [i64_value(source_generation)?]).map_err(|error| StorageError::sqlite("discard staged transcript workflows", error))?;
                    transaction.execute("DELETE FROM pod0_domain_cutovers WHERE domain='transcript_workflows' AND source_generation=?1 AND state='staged'",
                        [i64_value(source_generation)?]).map_err(|error| StorageError::sqlite("discard transcript workflow cutover", error))?;
                    transaction.execute("DELETE FROM pod0_transcript_workflow_imports WHERE singleton=1 AND source_generation=?1",
                        [i64_value(source_generation)?]).map_err(|error| StorageError::sqlite("discard transcript workflow import", error))?;
                    Ok(true)
                }
            TranscriptWorkflowAuthorityState::NotStarted => Ok(false),
            _ => Err(StorageError::TranscriptWorkflowConflict),
        })
    }

    pub fn export_legacy_transcript_workflow_rollback(
        &self,
    ) -> Result<TranscriptWorkflowRollbackExport, StorageError> {
        self.read(|connection| {
            if read_authority(connection)?.is_authoritative() {
                return Err(StorageError::TranscriptWorkflowConflict);
            }
            super::cutover_stage::read_rollback_export(connection)
        })
    }
}
