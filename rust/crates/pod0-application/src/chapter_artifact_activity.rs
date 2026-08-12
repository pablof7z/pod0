use pod0_domain::{CommandId, EpisodeId, StateRevision};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    ChapterTransition, DomainTransitionKind, DurableExternalEffectRequest,
    DurableInternalCommandRequest, NonEmptyActivityFacts, RequestDisposition,
    RequestRejectionReason, TransitionPlan, TransitionPlanError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChapterArtifactActivityInput {
    pub command_id: CommandId,
    pub episode_id: EpisodeId,
    pub current_selection_revision: StateRevision,
    pub expected_selection_revision: StateRevision,
    pub legacy_replay: bool,
    pub artifact_is_valid: bool,
    pub transcript_provenance_is_current: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChapterArtifactMutation {
    Commit,
    RecordRejection,
    LegacyDuplicate,
}

pub type ChapterArtifactPlan = TransitionPlan<
    ChapterArtifactMutation,
    DurableExternalEffectRequest,
    DurableInternalCommandRequest,
>;

pub fn plan_chapter_artifact_commit(
    input: ChapterArtifactActivityInput,
) -> Result<ChapterArtifactPlan, TransitionPlanError> {
    let identity = crate::CommandActivityIdentity::new(input.command_id);
    let transaction_id = identity.transaction_id();
    let subject = ActivitySubject::Episode {
        episode_id: input.episode_id,
    };
    let (mutation, disposition) = if input.legacy_replay {
        (
            ChapterArtifactMutation::LegacyDuplicate,
            RequestDisposition::Duplicate,
        )
    } else if !input.artifact_is_valid {
        (
            ChapterArtifactMutation::RecordRejection,
            RequestDisposition::Rejected {
                reason: RequestRejectionReason::Invalid,
            },
        )
    } else if !input.transcript_provenance_is_current
        || input.expected_selection_revision != input.current_selection_revision
    {
        (
            ChapterArtifactMutation::RecordRejection,
            RequestDisposition::Rejected {
                reason: RequestRejectionReason::RevisionConflict,
            },
        )
    } else {
        (
            ChapterArtifactMutation::Commit,
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
    let facts = if mutation == ChapterArtifactMutation::Commit {
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
                    kind: DomainTransitionKind::Chapter(kind),
                    previous_revision: input.current_selection_revision,
                    committed_revision: committed,
                },
            )
        };
        NonEmptyActivityFacts::from_head_and_tail(
            base(0, ActivityFact::RequestDisposition { disposition }),
            vec![
                transition(1, ChapterTransition::ArtifactAdopted),
                transition(2, ChapterTransition::SelectionChanged),
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
