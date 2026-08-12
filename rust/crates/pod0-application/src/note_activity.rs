use pod0_domain::{CommandId, EpisodeId, NoteId, StateRevision};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    DomainTransitionKind, DurableExternalEffectRequest, DurableInternalCommandRequest,
    NonEmptyActivityFacts, RequestDisposition, TransitionPlan, TransitionPlanError,
    UserArtifactTransition,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoteCreateActivityInput {
    pub command_id: CommandId,
    pub note_id: NoteId,
    pub episode_id: Option<EpisodeId>,
    pub current_revision: StateRevision,
    pub committed_revision: StateRevision,
    pub disposition: RequestDisposition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoteCreateMutation {
    Apply,
    None,
}

pub type NoteCreatePlan =
    TransitionPlan<NoteCreateMutation, DurableExternalEffectRequest, DurableInternalCommandRequest>;

pub fn plan_note_create(
    input: NoteCreateActivityInput,
) -> Result<NoteCreatePlan, TransitionPlanError> {
    let identity = crate::CommandActivityIdentity::new(input.command_id);
    let transaction_id = identity.transaction_id();
    let base = |ordinal, fact| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: identity.correlation_id(),
        caused_by_activity_id: None,
        command_id: Some(input.command_id),
        host_request_id: None,
        actor: ActivityActor::User,
        origin: ActivityOrigin::UserInterface,
        subject: ActivitySubject::Note {
            note_id: input.note_id,
        },
        episode_id: input.episode_id,
        fact,
    };
    let accepted = input.disposition == RequestDisposition::Accepted;
    if accepted && input.committed_revision.value <= input.current_revision.value {
        return Err(TransitionPlanError::DispositionRequiresTransition);
    }
    let facts = if accepted {
        NonEmptyActivityFacts::from_head_and_tail(
            base(
                0,
                ActivityFact::RequestDisposition {
                    disposition: input.disposition,
                },
            ),
            vec![base(
                1,
                ActivityFact::DomainTransition {
                    kind: DomainTransitionKind::UserArtifact(UserArtifactTransition::NoteChanged),
                    previous_revision: input.current_revision,
                    committed_revision: input.committed_revision,
                },
            )],
        )
    } else {
        NonEmptyActivityFacts::new(base(
            0,
            ActivityFact::RequestDisposition {
                disposition: input.disposition,
            },
        ))
    };
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        if accepted {
            NoteCreateMutation::Apply
        } else {
            NoteCreateMutation::None
        },
        facts,
        Vec::new(),
        Vec::new(),
    )
}
