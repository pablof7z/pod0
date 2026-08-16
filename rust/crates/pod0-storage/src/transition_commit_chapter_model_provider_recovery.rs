use pod0_application::{
    ActivityActor, ActivityOrigin, ChapterTransition, ChapterWorkflowActivityInput,
    ChapterWorkflowEffectAuthorization, ChapterWorkflowExecution, ChapterWorkflowMutation,
    ExternalEffectKind, RequestDisposition, plan_chapter_workflow_activity,
};
use pod0_domain::{CommandId, ContentDigest, EpisodeId, StateRevision, UnixTimestampMilliseconds};
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use crate::{
    LibraryStore, ModelChapterWorkflowRecord, ModelChapterWorkflowState, StorageError,
    TransitionIngress, TransitionIngressKind,
};

impl LibraryStore {
    pub fn authorize_model_chapter_provider_recovery(
        &self,
        episode_id: EpisodeId,
        observed_at_ms: i64,
    ) -> Result<ModelChapterWorkflowRecord, StorageError> {
        if has_active_recovery(self.path(), episode_id)? {
            return self
                .model_chapter_workflow(episode_id)?
                .ok_or(StorageError::ChapterWorkflowNotFound);
        }
        commit(self.path(), episode_id, observed_at_ms)
    }
}

fn has_active_recovery(path: &std::path::Path, episode_id: EpisodeId) -> Result<bool, StorageError> {
    let connection = crate::migration_db::open_connection(path, true)?;
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM pod0_effect_intents WHERE episode_id=?1
             AND state_code IN(1,2)
             AND json_type(request_json,'$.execution.ModelChapter.request.action.Recover')
                 IS NOT NULL)",
            [episode_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read model provider recovery effect", error))
}

fn commit(
    path: &std::path::Path,
    episode_id: EpisodeId,
    observed_at_ms: i64,
) -> Result<ModelChapterWorkflowRecord, StorageError> {
    TransitionCommit::open(path)?.commit_resolved_ingress_with(
        UnixTimestampMilliseconds::new(observed_at_ms),
        |transaction| {
            let current = current(transaction, episode_id)?;
            Ok(TransitionIngress {
                kind: TransitionIngressKind::Recovery,
                id: identity(&current).into_bytes(),
                fingerprint: fingerprint(&current),
            })
        },
        |transaction| {
            let current = current(transaction, episode_id)?;
            let execution = crate::chapter_effect_request::model_recovery_request(
                &current,
                current
                    .provider_operation_id
                    .clone()
                    .ok_or(StorageError::ChapterWorkflowConflict)?,
                current.provider_status.clone(),
            )?;
            plan_chapter_workflow_activity(ChapterWorkflowActivityInput {
                identity_command_id: identity(&current),
                command_id: current.command_id,
                episode_id,
                current_revision: current.workflow_revision,
                disposition: RequestDisposition::Accepted,
                transition: Some(ChapterTransition::ModelWorkflowStateChanged),
                effect: Some(ChapterWorkflowEffectAuthorization {
                    not_before: None,
                    deadline_at: None,
                    execution: ChapterWorkflowExecution::Model(execution),
                }),
                effect_kind: ExternalEffectKind::ModelChapterProvider,
                actor: ActivityActor::Recovery,
                origin: ActivityOrigin::Recovery,
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, mutation| {
            if mutation != ChapterWorkflowMutation::Apply {
                return Err(StorageError::InvalidActivity);
            }
            super::chapter_model_cancel::retire_model_effects(transaction, episode_id)?;
            transaction
                .execute(
                    "UPDATE pod0_model_chapter_workflows SET workflow_revision=workflow_revision+1,
                     updated_at_ms=?1 WHERE episode_id=?2 AND workflow_revision=?3
                     AND state='provider_accepted'",
                    rusqlite::params![
                        observed_at_ms,
                        episode_id.into_bytes().as_slice(),
                        i64::try_from(expected.value).map_err(|_| StorageError::InvalidActivity)?
                    ],
                )
                .map_err(|error| {
                    StorageError::sqlite("authorize model provider recovery", error)
                })?;
            if transaction.changes() != 1 {
                return Err(StorageError::RevisionConflict);
            }
            Ok(StateRevision::new(
                expected
                    .value
                    .checked_add(1)
                    .ok_or(StorageError::InvalidActivity)?,
            ))
        },
    )?;
    LibraryStore::open_authoritative(path)?
        .model_chapter_workflow(episode_id)?
        .ok_or(StorageError::ChapterWorkflowNotFound)
}

fn current(
    transaction: &rusqlite::Transaction<'_>,
    episode_id: EpisodeId,
) -> Result<ModelChapterWorkflowRecord, StorageError> {
    crate::model_chapter_workflow::read::read_workflow(transaction, episode_id)?
        .filter(|record| record.state == ModelChapterWorkflowState::ProviderAccepted)
        .ok_or(StorageError::ChapterWorkflowConflict)
}

fn identity(record: &ModelChapterWorkflowRecord) -> CommandId {
    let mut hash = Sha256::new();
    hash.update(b"pod0/chapter/model-provider-recovery/v1");
    hash.update(record.request_id.expect("accepted request").into_bytes());
    hash.update(record.workflow_revision.value.to_be_bytes());
    let digest: [u8; 32] = hash.finalize().into();
    CommandId::from_bytes(digest[..16].try_into().unwrap())
}

fn fingerprint(record: &ModelChapterWorkflowRecord) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0/chapter/model-provider-recovery-ingress/v1");
    hash.update(record.episode_id.into_bytes());
    hash.update(
        record
            .provider_operation_id
            .as_deref()
            .unwrap_or_default()
            .as_bytes(),
    );
    ContentDigest::from_bytes(hash.finalize().into())
}
