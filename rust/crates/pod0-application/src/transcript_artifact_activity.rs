use pod0_domain::{CommandId, EpisodeId, StateRevision};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    DomainTransitionKind, DurableExternalEffectRequest, DurableInternalCommandRequest,
    NonEmptyActivityFacts, RequestDisposition, RequestRejectionReason, TranscriptTransition,
    TransitionPlan, TransitionPlanError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TranscriptArtifactActivityInput {
    pub command_id: CommandId,
    pub episode_id: EpisodeId,
    pub current_selection_revision: StateRevision,
    pub expected_selection_revision: StateRevision,
    pub legacy_replay: bool,
    pub artifact_is_valid: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TranscriptArtifactMutation {
    Commit,
    RecordRejection,
    LegacyDuplicate,
}

pub type TranscriptArtifactPlan = TransitionPlan<
    TranscriptArtifactMutation,
    DurableExternalEffectRequest,
    DurableInternalCommandRequest,
>;

pub fn plan_transcript_artifact_commit(
    input: TranscriptArtifactActivityInput,
) -> Result<TranscriptArtifactPlan, TransitionPlanError> {
    let identity = crate::CommandActivityIdentity::new(input.command_id);
    let transaction_id = identity.transaction_id();
    let subject = ActivitySubject::Episode {
        episode_id: input.episode_id,
    };
    let (mutation, disposition) = if input.legacy_replay {
        (
            TranscriptArtifactMutation::LegacyDuplicate,
            RequestDisposition::Duplicate,
        )
    } else if !input.artifact_is_valid {
        (
            TranscriptArtifactMutation::RecordRejection,
            RequestDisposition::Rejected {
                reason: RequestRejectionReason::Invalid,
            },
        )
    } else if input.expected_selection_revision != input.current_selection_revision {
        (
            TranscriptArtifactMutation::RecordRejection,
            RequestDisposition::Rejected {
                reason: RequestRejectionReason::RevisionConflict,
            },
        )
    } else {
        (
            TranscriptArtifactMutation::Commit,
            RequestDisposition::Accepted,
        )
    };
    let base = |ordinal, fact| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: identity.correlation_id(),
        caused_by_activity_id: None,
        command_id: Some(input.command_id),
        host_request_id: None,
        actor: ActivityActor::User,
        origin: ActivityOrigin::UserInterface,
        subject,
        episode_id: Some(input.episode_id),
        fact,
    };
    let facts = if mutation == TranscriptArtifactMutation::Commit {
        let committed = StateRevision::new(
            input
                .current_selection_revision
                .value
                .checked_add(1)
                .ok_or(TransitionPlanError::RevisionExhausted)?,
        );
        let transition = |ordinal, kind| {
            base(
                ordinal,
                ActivityFact::DomainTransition {
                    kind: DomainTransitionKind::Transcript(kind),
                    previous_revision: input.current_selection_revision,
                    committed_revision: committed,
                },
            )
        };
        NonEmptyActivityFacts::from_head_and_tail(
            base(0, ActivityFact::RequestDisposition { disposition }),
            vec![
                transition(1, TranscriptTransition::ArtifactAdopted),
                transition(2, TranscriptTransition::SelectionChanged),
            ],
        )
    } else {
        NonEmptyActivityFacts::new(base(0, ActivityFact::RequestDisposition { disposition }))
    };
    TransitionPlan::new(
        transaction_id,
        input.current_selection_revision,
        mutation,
        facts,
        Vec::new(),
        Vec::new(),
    )
}
