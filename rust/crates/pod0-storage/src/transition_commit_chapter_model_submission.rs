use pod0_application::{
    ActivityActor, ActivityOrigin, ChapterTransition, ChapterWorkflowActivityInput,
    ChapterWorkflowEffectAuthorization, ChapterWorkflowExecution, ChapterWorkflowMutation,
    ExternalEffectKind,
    RequestDisposition, plan_chapter_workflow_activity,
};
use pod0_domain::{CommandId, ContentDigest, UnixTimestampMilliseconds};
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use crate::{
    ModelChapterSubmissionClaim, ModelChapterSubmissionClaimInput, ModelChapterWorkflowState,
    StorageError, TransitionIngress, TransitionIngressKind,
};

pub(crate) fn commit_model_chapter_submission(
    path: &std::path::Path,
    input: ModelChapterSubmissionClaimInput,
) -> Result<ModelChapterSubmissionClaim, StorageError> {
    let receipt = TransitionCommit::open(path)?.commit_planned_with(
        TransitionIngress {
            kind: TransitionIngressKind::ScheduledWake,
            id: input.request_id.into_bytes(),
            fingerprint: fingerprint(&input),
        },
        UnixTimestampMilliseconds::new(input.now_ms),
        |transaction| {
            let current = crate::model_chapter_workflow::exact_claim_record(transaction, &input)?;
            if current.state.may_have_submitted()
                || !matches!(
                    current.state,
                    ModelChapterWorkflowState::Requested
                        | ModelChapterWorkflowState::RetryScheduled
                )
                || current
                    .not_before_ms
                    .is_some_and(|value| value > input.now_ms)
                || current
                    .deadline_at_ms
                    .is_none_or(|value| value < input.now_ms)
                || input.now_ms < 0
            {
                return Err(StorageError::ChapterWorkflowConflict);
            }
            plan_chapter_workflow_activity(ChapterWorkflowActivityInput {
                identity_command_id: submission_identity(&input),
                command_id: current.command_id,
                episode_id: input.episode_id,
                current_revision: current.workflow_revision,
                disposition: RequestDisposition::Accepted,
                transition: Some(ChapterTransition::ModelWorkflowStateChanged),
                effect: Some(ChapterWorkflowEffectAuthorization {
                    not_before: None,
                    deadline_at: current.deadline_at_ms.map(UnixTimestampMilliseconds::new),
                    execution: ChapterWorkflowExecution::Model(
                        crate::chapter_effect_request::model_execution_request(&current)?,
                    ),
                }),
                effect_kind: ExternalEffectKind::ModelChapterProvider,
                actor: ActivityActor::System,
                origin: ActivityOrigin::ScheduledWork,
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, mutation| {
            if mutation != ChapterWorkflowMutation::Apply {
                return Err(StorageError::InvalidActivity);
            }
            let ModelChapterSubmissionClaim::Authorized(record) =
                crate::model_chapter_workflow::apply_model_chapter_submission_claim(
                    transaction,
                    &input,
                )?
            else {
                return Err(StorageError::ChapterWorkflowConflict);
            };
            if record.workflow_revision.value != expected.value.saturating_add(1) {
                return Err(StorageError::RevisionConflict);
            }
            Ok(record.workflow_revision)
        },
    )?;
    let record = crate::LibraryStore::open_authoritative(path)?
        .model_chapter_workflow(input.episode_id)?
        .ok_or(StorageError::ChapterWorkflowNotFound)?;
    if receipt.replayed {
        Ok(ModelChapterSubmissionClaim::AlreadyClaimed(record))
    } else {
        Ok(ModelChapterSubmissionClaim::Authorized(record))
    }
}

fn submission_identity(input: &ModelChapterSubmissionClaimInput) -> CommandId {
    let mut hash = Sha256::new();
    hash.update(b"pod0/chapter/model-submission/v1");
    hash.update(input.episode_id.into_bytes());
    hash.update(input.request_id.into_bytes());
    hash.update(input.generation.to_be_bytes());
    hash.update(input.cancellation_id.into_bytes());
    hash.update(input.issued_revision.value.to_be_bytes());
    let digest: [u8; 32] = hash.finalize().into();
    CommandId::from_bytes(digest[..16].try_into().expect("fixed digest prefix"))
}

fn fingerprint(input: &ModelChapterSubmissionClaimInput) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0/chapter/model-submission-ingress/v1");
    hash.update(input.episode_id.into_bytes());
    hash.update(input.request_id.into_bytes());
    hash.update(input.generation.to_be_bytes());
    hash.update(input.cancellation_id.into_bytes());
    hash.update(input.issued_revision.value.to_be_bytes());
    ContentDigest::from_bytes(hash.finalize().into())
}
