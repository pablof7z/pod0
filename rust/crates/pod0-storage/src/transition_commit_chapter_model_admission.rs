use std::cell::RefCell;

use pod0_application::{
    ActivityActor, ActivityOrigin, ChapterTransition, ChapterWorkflowActivityInput,
    ChapterWorkflowMutation, ExternalEffectKind, RequestDisposition,
    plan_chapter_workflow_activity,
};
use pod0_domain::{ContentDigest, StateRevision, UnixTimestampMilliseconds};
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use crate::{
    ModelChapterDesiredPlan, ModelChapterEnsureInput, ModelChapterEnsureOutcome, StorageError,
    TransitionIngress, TransitionIngressKind,
};

pub(crate) fn commit_model_chapter_admission(
    path: &std::path::Path,
    input: ModelChapterEnsureInput,
) -> Result<ModelChapterEnsureOutcome, StorageError> {
    let outcome = RefCell::new(None);
    let receipt = TransitionCommit::open(path)?.commit_planned_with(
        TransitionIngress {
            kind: TransitionIngressKind::ApplicationCommand,
            id: input.command_id.into_bytes(),
            fingerprint: fingerprint(&input),
        },
        UnixTimestampMilliseconds::new(input.now_ms),
        |transaction| {
            let (current_revision, changes) =
                crate::model_chapter_workflow::model_chapter_admission_state(transaction, &input)?;
            plan_chapter_workflow_activity(ChapterWorkflowActivityInput {
                identity_command_id: input.command_id,
                command_id: input.command_id,
                episode_id: input.episode_id,
                current_revision,
                disposition: if changes {
                    RequestDisposition::Accepted
                } else {
                    RequestDisposition::NoSemanticChange
                },
                transition: changes.then_some(ChapterTransition::ModelWorkflowStateChanged),
                effect: None,
                effect_kind: ExternalEffectKind::ModelChapterProvider,
                actor: ActivityActor::User,
                origin: ActivityOrigin::UserInterface,
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, mutation| {
            let applied =
                crate::model_chapter_workflow::apply_model_chapter_ensure(transaction, &input)?;
            let revision = outcome_revision(&applied);
            match mutation {
                ChapterWorkflowMutation::Apply
                    if revision.value == expected.value.saturating_add(1) => {}
                ChapterWorkflowMutation::RecordNoChange if revision == expected => {}
                _ => return Err(StorageError::RevisionConflict),
            }
            *outcome.borrow_mut() = Some(applied);
            Ok(revision)
        },
    )?;
    if receipt.replayed {
        let record = crate::LibraryStore::open_authoritative(path)?
            .model_chapter_workflow(input.episode_id)?
            .ok_or(StorageError::ChapterWorkflowNotFound)?;
        return Ok(ModelChapterEnsureOutcome::Existing(record));
    }
    outcome.into_inner().ok_or(StorageError::InvalidActivity)
}

fn outcome_revision(outcome: &ModelChapterEnsureOutcome) -> StateRevision {
    match outcome {
        ModelChapterEnsureOutcome::Changed { record, .. }
        | ModelChapterEnsureOutcome::Existing(record) => record.workflow_revision,
    }
}

pub(super) fn fingerprint(input: &ModelChapterEnsureInput) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0/chapter/model-admission/v1");
    hash.update(input.episode_id.into_bytes());
    hash.update(input.configured_model.as_bytes());
    hash.update(input.cancellation_id.into_bytes());
    hash.update(input.issued_revision.value.to_be_bytes());
    hash.update(input.max_attempts.to_be_bytes());
    hash.update(
        input
            .force_retry_from_revision
            .map_or(u64::MAX, |value| value.value)
            .to_be_bytes(),
    );
    hash_plan(&mut hash, &input.desired_plan);
    ContentDigest::from_bytes(hash.finalize().into())
}

fn hash_plan(hash: &mut Sha256, plan: &ModelChapterDesiredPlan) {
    match plan {
        ModelChapterDesiredPlan::AwaitingTranscript => hash.update([1]),
        ModelChapterDesiredPlan::AwaitingPublisher => hash.update([2]),
        ModelChapterDesiredPlan::Current {
            artifact_id,
            selection_revision,
        } => {
            hash.update([3]);
            hash.update(artifact_id.into_bytes());
            hash.update(selection_revision.value.to_be_bytes());
        }
        ModelChapterDesiredPlan::PreserveAgentComposed {
            artifact_id,
            selection_revision,
        } => {
            hash.update([4]);
            hash.update(artifact_id.into_bytes());
            hash.update(selection_revision.value.to_be_bytes());
        }
        ModelChapterDesiredPlan::Blocked {
            failure_code,
            failure_detail,
        } => {
            hash.update([5]);
            hash.update(failure_code.as_bytes());
            hash.update([0]);
            hash.update(failure_detail.as_deref().unwrap_or_default().as_bytes());
        }
        ModelChapterDesiredPlan::Ready(request) => {
            hash.update([6]);
            hash.update(request.request_fingerprint.into_bytes());
        }
    }
}
