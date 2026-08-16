use pod0_application::{
    ActivityActor, ActivityOrigin, ChapterTransition, ChapterWorkflowActivityInput,
    ChapterWorkflowMutation, ExternalEffectKind, RequestDisposition,
    plan_chapter_workflow_activity,
};
use pod0_domain::{CommandId, ContentDigest, EpisodeId, HostRequestId, UnixTimestampMilliseconds};
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use crate::{StorageError, TransitionIngress, TransitionIngressKind};

pub(crate) fn commit_model_chapter_ambiguity_recovery(
    path: &std::path::Path,
    episode_id: EpisodeId,
    request_id: HostRequestId,
    observed_at_ms: i64,
) -> Result<(), StorageError> {
    let identity = recovery_identity(request_id);
    TransitionCommit::open(path)?.commit_planned_with(
        TransitionIngress {
            kind: TransitionIngressKind::Recovery,
            id: identity.into_bytes(),
            fingerprint: recovery_fingerprint(episode_id, request_id),
        },
        UnixTimestampMilliseconds::new(observed_at_ms),
        |transaction| {
            let current =
                crate::model_chapter_workflow::read::read_workflow(transaction, episode_id)?
                    .ok_or(StorageError::ChapterWorkflowNotFound)?;
            if current.request_id != Some(request_id)
                || current.state != crate::ModelChapterWorkflowState::SubmissionAuthorized
            {
                return Err(StorageError::ChapterWorkflowConflict);
            }
            plan_chapter_workflow_activity(ChapterWorkflowActivityInput {
                identity_command_id: identity,
                command_id: current.command_id,
                episode_id,
                current_revision: current.workflow_revision,
                disposition: RequestDisposition::Accepted,
                transition: Some(ChapterTransition::ModelWorkflowStateChanged),
                effect: None,
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
            let updated = crate::LibraryStore::apply_model_chapter_ambiguity_recovery(
                transaction,
                episode_id,
                request_id,
                observed_at_ms,
            )?;
            if updated.workflow_revision.value != expected.value.saturating_add(1) {
                return Err(StorageError::RevisionConflict);
            }
            super::chapter_model_cancel::retire_model_effects(transaction, episode_id)?;
            Ok(updated.workflow_revision)
        },
    )?;
    Ok(())
}

fn recovery_identity(request_id: HostRequestId) -> CommandId {
    let mut hash = Sha256::new();
    hash.update(b"pod0/chapter/model-ambiguity-recovery/v1");
    hash.update(request_id.into_bytes());
    let digest: [u8; 32] = hash.finalize().into();
    CommandId::from_bytes(digest[..16].try_into().expect("fixed digest prefix"))
}

fn recovery_fingerprint(episode_id: EpisodeId, request_id: HostRequestId) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0/chapter/model-ambiguity-recovery-ingress/v1");
    hash.update(episode_id.into_bytes());
    hash.update(request_id.into_bytes());
    ContentDigest::from_bytes(hash.finalize().into())
}
