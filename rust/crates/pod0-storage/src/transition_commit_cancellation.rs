use pod0_application::{
    CancellationActivityInput, CancellationEffectTarget, CoreFailure, CoreFailureCode,
    DurableEffectExecution, DurableExternalEffectRequest, RecallStage, Retryability, UserAction,
    plan_cancellation_activity,
};
use pod0_domain::{CancellationId, CommandId, ContentDigest, UnixTimestampMilliseconds};

use super::TransitionCommit;
use crate::{LibraryStore, StorageError, TransitionIngress, TransitionIngressKind};

#[path = "transition_commit_cancellation_identity.rs"]
mod cancellation_identity;
#[path = "transition_commit_chapter_cancellation_hook.rs"]
mod chapter_cancellation_hook;

impl LibraryStore {
    pub fn cancel_durable_effects(
        &self,
        command_id: CommandId,
        fingerprint: ContentDigest,
        cancellation_id: CancellationId,
        committed_at: UnixTimestampMilliseconds,
    ) -> Result<crate::CommitReceipt, StorageError> {
        commit(
            self.path(),
            command_id,
            fingerprint,
            cancellation_id,
            committed_at,
            false,
        )
    }

    pub fn cancel_durable_lifecycle_wakes(
        &self,
        command_id: CommandId,
        fingerprint: ContentDigest,
        cancellation_id: CancellationId,
        committed_at: UnixTimestampMilliseconds,
    ) -> Result<crate::CommitReceipt, StorageError> {
        commit(
            self.path(),
            command_id,
            fingerprint,
            cancellation_id,
            committed_at,
            true,
        )
    }
}

fn commit(
    path: &std::path::Path,
    command_id: CommandId,
    fingerprint: ContentDigest,
    cancellation_id: CancellationId,
    committed_at: UnixTimestampMilliseconds,
    lifecycle_only: bool,
) -> Result<crate::CommitReceipt, StorageError> {
    TransitionCommit::open(path)?.commit_planned_with(
        TransitionIngress {
            kind: TransitionIngressKind::ApplicationCommand,
            id: command_id.into_bytes(),
            fingerprint,
        },
        committed_at,
        |transaction| {
            let effects = cancellable_effects(transaction, cancellation_id, lifecycle_only)?;
            let targets = effects.iter().map(|effect| effect.target).collect();
            let updates_recall = effects.iter().any(|effect| effect.updates_recall);
            let updates_chapter = effects.iter().any(|effect| effect.updates_chapter);
            let intent_ids: Vec<[u8; 16]> =
                effects.into_iter().map(|effect| effect.intent_id).collect();
            let current = if updates_recall {
                core_revision(transaction)?
            } else {
                pod0_domain::StateRevision::INITIAL
            };
            plan_cancellation_activity(CancellationActivityInput {
                command_id,
                current_revision: current,
                targets,
            })
            .map(|plan| plan.map_mutation(|()| (intent_ids, updates_recall, updates_chapter)))
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, (intent_ids, updates_recall, updates_chapter)| {
            for intent_id in intent_ids {
                transaction
                    .execute(
                        "UPDATE pod0_effect_attempts SET state_code=4 \
                         WHERE intent_id=?1 AND state_code IN(1,2)",
                        [intent_id.as_slice()],
                    )
                    .map_err(|error| {
                        StorageError::sqlite("supersede durable effect attempts", error)
                    })?;
                let changed = transaction
                    .execute(
                        "UPDATE pod0_effect_intents SET state_code=4 \
                         WHERE intent_id=?1 AND state_code IN(1,2)",
                        [intent_id.as_slice()],
                    )
                    .map_err(|error| {
                        StorageError::sqlite("supersede durable effect intent", error)
                    })?;
                if changed != 1 {
                    return Err(StorageError::RevisionConflict);
                }
            }
            if updates_chapter {
                chapter_cancellation_hook::cancel_chapter_workflows(
                    transaction,
                    cancellation_id,
                    committed_at,
                )?;
            }
            if !updates_recall {
                return Ok(expected);
            }
            let committed = crate::library_store::advance_playback_revision(transaction)?;
            if committed.value != expected.value.saturating_add(1) {
                return Err(StorageError::RevisionConflict);
            }
            cancel_recall_workflows(transaction, cancellation_id, committed, committed_at)?;
            Ok(committed)
        },
    )
}

#[derive(Clone, Copy)]
struct CancellableEffect {
    intent_id: [u8; 16],
    target: CancellationEffectTarget,
    updates_recall: bool,
    updates_chapter: bool,
}

fn cancellable_effects(
    transaction: &rusqlite::Transaction<'_>,
    cancellation_id: CancellationId,
    lifecycle_only: bool,
) -> Result<Vec<CancellableEffect>, StorageError> {
    let mut statement = transaction
        .prepare(
            "SELECT intent_id,request_json FROM pod0_effect_intents \
             WHERE state_code IN(1,2) ORDER BY committed_at_ms,intent_id",
        )
        .map_err(|error| StorageError::sqlite("read cancellable durable effects", error))?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| StorageError::sqlite("query cancellable durable effects", error))?;
    let mut effects = Vec::new();
    for row in rows {
        let (intent, payload) =
            row.map_err(|error| StorageError::sqlite("decode cancellable durable effect", error))?;
        let request: DurableExternalEffectRequest =
            serde_json::from_str(&payload).map_err(|_| StorageError::InvalidActivity)?;
        if lifecycle_only && !matches!(&request.execution, DurableEffectExecution::Lifecycle { .. })
        {
            continue;
        }
        let Some((effect_cancellation, host_request_id)) =
            cancellation_identity::cancellation_identity(&request.execution)
        else {
            continue;
        };
        if effect_cancellation != cancellation_id {
            continue;
        }
        effects.push(CancellableEffect {
            intent_id: intent
                .try_into()
                .map_err(|_| StorageError::InvalidActivity)?,
            target: CancellationEffectTarget {
                subject: request.subject,
                episode_id: request.episode_id,
                host_request_id,
                cancellation_id,
            },
            updates_recall: matches!(
                request.execution,
                DurableEffectExecution::RecallQuery { .. }
                    | DurableEffectExecution::RecallIndexCutover { .. }
            ),
            updates_chapter: matches!(
                request.execution,
                DurableEffectExecution::PublisherChapter { .. }
                    | DurableEffectExecution::ModelChapter { .. }
            ),
        });
    }
    Ok(effects)
}

fn cancel_recall_workflows(
    transaction: &rusqlite::Transaction<'_>,
    cancellation_id: CancellationId,
    committed: pod0_domain::StateRevision,
    committed_at: UnixTimestampMilliseconds,
) -> Result<(), StorageError> {
    let failure = serde_json::to_string(&CoreFailure {
        code: CoreFailureCode::Cancelled,
        safe_detail: None,
        retryability: Retryability::Never,
        user_action: UserAction::None,
    })
    .map_err(|_| StorageError::InvalidActivity)?;
    let cancelled = serde_json::to_string(&RecallStage::Cancelled)
        .map_err(|_| StorageError::InvalidActivity)?;
    let revision = i64::try_from(committed.value).map_err(|_| StorageError::InvalidActivity)?;
    transaction
        .execute(
            "UPDATE pod0_recall_queries SET revision=?1,stage_json=?2,evidence_json='[]',\
         failure_json=?3,updated_at_ms=?4 WHERE cancellation_id=?5 AND \
         (stage_json='\"Queued\"' OR json_type(stage_json,'$.Running') IS NOT NULL)",
            rusqlite::params![
                revision,
                cancelled,
                failure,
                committed_at.value,
                cancellation_id.into_bytes().as_slice()
            ],
        )
        .map_err(|error| StorageError::sqlite("cancel recall query workflow", error))?;
    transaction
        .execute(
            "UPDATE pod0_recall_index_cutover_workflow SET revision=?1,stage='cancelled',\
         removed_file_count=NULL,updated_at_ms=?2 WHERE cancellation_id=?3 AND \
         stage='awaiting_host'",
            rusqlite::params![
                revision,
                committed_at.value,
                cancellation_id.into_bytes().as_slice()
            ],
        )
        .map_err(|error| StorageError::sqlite("cancel recall cutover workflow", error))?;
    Ok(())
}

fn core_revision(
    connection: &rusqlite::Connection,
) -> Result<pod0_domain::StateRevision, StorageError> {
    let value: i64 = connection
        .query_row(
            "SELECT state_revision FROM pod0_playback_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read cancellation core revision", error))?;
    Ok(pod0_domain::StateRevision::new(
        u64::try_from(value).map_err(|_| StorageError::InvalidActivity)?,
    ))
}
