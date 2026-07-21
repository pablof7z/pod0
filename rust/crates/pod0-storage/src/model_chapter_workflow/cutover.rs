use pod0_domain::{
    CancellationId, ChapterArtifactId, CommandId, ContentDigest, EpisodeId, StateRevision,
};
use rusqlite::{OptionalExtension, Transaction, params};

use super::cutover_adoption::{migrated_record, validate_cutover_input};
use super::model::StoredModelChapterRequest;
use super::persist::persist_workflow;
use super::support::i64_value;
use crate::{LibraryStore, StorageError};

pub(super) const CUTOVER_DOMAIN: &str = "model_chapter_workflows";
pub(super) const MAX_CUTOVER_ENTRIES: usize = 20_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelChapterWorkflowAuthorityState {
    NotStarted,
    Staged { source_generation: u64 },
    Authoritative { source_generation: u64 },
}

impl ModelChapterWorkflowAuthorityState {
    pub const fn is_authoritative(self) -> bool {
        matches!(self, Self::Authoritative { .. })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LegacyModelChapterWorkflowDisposition {
    Succeeded {
        artifact_id: ChapterArtifactId,
        content_digest: ContentDigest,
        integrity_digest: ContentDigest,
        selection_revision: StateRevision,
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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyModelChapterWorkflowEntry {
    pub episode_id: EpisodeId,
    pub configured_model: String,
    pub request: StoredModelChapterRequest,
    pub disposition: LegacyModelChapterWorkflowDisposition,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyModelChapterWorkflowCutoverInput {
    pub source_generation: u64,
    pub entries: Vec<LegacyModelChapterWorkflowEntry>,
    pub command_id: CommandId,
    pub cancellation_id: CancellationId,
    pub issued_revision: StateRevision,
    pub now_ms: i64,
    pub max_attempts: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LegacyModelChapterWorkflowCutoverReport {
    pub state: ModelChapterWorkflowAuthorityState,
    pub adopted_succeeded: u32,
    pub adopted_ambiguous: u32,
}

impl LibraryStore {
    pub fn model_chapter_workflow_authority(
        &self,
    ) -> Result<ModelChapterWorkflowAuthorityState, StorageError> {
        self.read(read_authority)
    }

    pub fn stage_legacy_model_chapter_workflow_cutover(
        &self,
        input: LegacyModelChapterWorkflowCutoverInput,
    ) -> Result<LegacyModelChapterWorkflowCutoverReport, StorageError> {
        validate_cutover_input(&input)?;
        self.write(|transaction| {
            match read_authority(transaction)? {
                ModelChapterWorkflowAuthorityState::NotStarted => {}
                ModelChapterWorkflowAuthorityState::Staged { source_generation }
                    if source_generation == input.source_generation =>
                {
                    return report_for_existing(transaction, source_generation, false);
                }
                ModelChapterWorkflowAuthorityState::Authoritative { source_generation }
                    if source_generation == input.source_generation =>
                {
                    return report_for_existing(transaction, source_generation, true);
                }
                _ => return Err(StorageError::ChapterWorkflowConflict),
            }
            let existing: bool = transaction
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM pod0_model_chapter_workflows LIMIT 1)",
                    [],
                    |row| row.get(0),
                )
                .map_err(|error| {
                    StorageError::sqlite("inspect model chapter workflows before cutover", error)
                })?;
            if existing {
                return Err(StorageError::ChapterWorkflowConflict);
            }

            let mut adopted_succeeded = 0_u32;
            let mut adopted_ambiguous = 0_u32;
            for entry in &input.entries {
                let record = migrated_record(transaction, &input, entry)?;
                match &entry.disposition {
                    LegacyModelChapterWorkflowDisposition::Succeeded { .. } => {
                        adopted_succeeded = adopted_succeeded
                            .checked_add(1)
                            .ok_or(StorageError::ChapterWorkflowConflict)?;
                    }
                    LegacyModelChapterWorkflowDisposition::Ambiguous => {
                        adopted_ambiguous = adopted_ambiguous
                            .checked_add(1)
                            .ok_or(StorageError::ChapterWorkflowConflict)?;
                    }
                    LegacyModelChapterWorkflowDisposition::Blocked { .. }
                    | LegacyModelChapterWorkflowDisposition::Failed { .. }
                    | LegacyModelChapterWorkflowDisposition::Cancelled { .. } => {}
                }
                persist_workflow(transaction, &record)?;
            }
            transaction
                .execute(
                    "INSERT INTO pod0_domain_cutovers(domain,state,source_generation,\
                     core_revision,committed_at_ms) VALUES(?1,'staged',?2,?3,?4)",
                    params![
                        CUTOVER_DOMAIN,
                        i64_value(input.source_generation)?,
                        i64_value(input.issued_revision.value)?,
                        input.now_ms,
                    ],
                )
                .map_err(|error| {
                    StorageError::sqlite("stage model chapter workflow cutover", error)
                })?;
            Ok(LegacyModelChapterWorkflowCutoverReport {
                state: ModelChapterWorkflowAuthorityState::Staged {
                    source_generation: input.source_generation,
                },
                adopted_succeeded,
                adopted_ambiguous,
            })
        })
    }

    pub fn commit_legacy_model_chapter_workflow_cutover(
        &self,
        source_generation: u64,
        committed_at_ms: i64,
    ) -> Result<ModelChapterWorkflowAuthorityState, StorageError> {
        if source_generation == 0 || committed_at_ms < 0 {
            return Err(StorageError::ChapterWorkflowConflict);
        }
        self.write(|transaction| match read_authority(transaction)? {
            ModelChapterWorkflowAuthorityState::Staged {
                source_generation: staged,
            } if staged == source_generation => {
                transaction
                    .execute(
                        "UPDATE pod0_domain_cutovers SET state='authoritative',\
                         committed_at_ms=?1 WHERE domain=?2 AND state='staged'\
                         AND source_generation=?3",
                        params![
                            committed_at_ms,
                            CUTOVER_DOMAIN,
                            i64_value(source_generation)?
                        ],
                    )
                    .map_err(|error| {
                        StorageError::sqlite("commit model chapter workflow cutover", error)
                    })?;
                if transaction.changes() != 1 {
                    return Err(StorageError::ChapterWorkflowConflict);
                }
                Ok(ModelChapterWorkflowAuthorityState::Authoritative { source_generation })
            }
            ModelChapterWorkflowAuthorityState::Authoritative {
                source_generation: current,
            } if current == source_generation => {
                Ok(ModelChapterWorkflowAuthorityState::Authoritative { source_generation })
            }
            _ => Err(StorageError::ChapterWorkflowConflict),
        })
    }
}

pub(super) fn read_authority(
    connection: &rusqlite::Connection,
) -> Result<ModelChapterWorkflowAuthorityState, StorageError> {
    let row: Option<(String, i64)> = connection
        .query_row(
            "SELECT state,source_generation FROM pod0_domain_cutovers WHERE domain=?1",
            [CUTOVER_DOMAIN],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read model chapter workflow authority", error))?;
    match row {
        None => Ok(ModelChapterWorkflowAuthorityState::NotStarted),
        Some((state, generation)) => {
            let source_generation =
                u64::try_from(generation).map_err(|_| StorageError::ChapterWorkflowConflict)?;
            match state.as_str() {
                "staged" => Ok(ModelChapterWorkflowAuthorityState::Staged { source_generation }),
                "authoritative" => {
                    Ok(ModelChapterWorkflowAuthorityState::Authoritative { source_generation })
                }
                _ => Err(StorageError::ChapterWorkflowConflict),
            }
        }
    }
}

fn report_for_existing(
    transaction: &Transaction<'_>,
    source_generation: u64,
    authoritative: bool,
) -> Result<LegacyModelChapterWorkflowCutoverReport, StorageError> {
    let adopted_succeeded = count_state(transaction, "succeeded")?;
    let adopted_ambiguous = count_state(transaction, "ambiguous")?;
    Ok(LegacyModelChapterWorkflowCutoverReport {
        state: if authoritative {
            ModelChapterWorkflowAuthorityState::Authoritative { source_generation }
        } else {
            ModelChapterWorkflowAuthorityState::Staged { source_generation }
        },
        adopted_succeeded,
        adopted_ambiguous,
    })
}

fn count_state(transaction: &Transaction<'_>, state: &str) -> Result<u32, StorageError> {
    let count: i64 = transaction
        .query_row(
            "SELECT COUNT(*) FROM pod0_model_chapter_workflows WHERE state=?1",
            [state],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("count migrated model chapter workflows", error))?;
    u32::try_from(count).map_err(|_| StorageError::ChapterWorkflowConflict)
}
