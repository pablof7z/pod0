use pod0_domain::{CommandId, StateRevision};

use crate::{
    ActivityActor, ActivityDomain, ActivityFact, ActivityFactDraft, ActivityOrigin,
    ActivitySubject, ChapterTransition, DomainTransitionKind, DurableExternalEffectRequest,
    DurableInternalCommandRequest, NonEmptyActivityFacts, RequestDisposition, TransitionPlan,
    TransitionPlanError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChapterCutoverActivityInput {
    pub command_id: CommandId,
    pub current_revision: StateRevision,
    pub committed_revision: StateRevision,
    pub disposition: RequestDisposition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChapterCutoverMutation {
    Apply,
    None,
}

pub type ChapterCutoverPlan = TransitionPlan<
    ChapterCutoverMutation,
    DurableExternalEffectRequest,
    DurableInternalCommandRequest,
>;

pub fn plan_chapter_cutover(
    input: ChapterCutoverActivityInput,
) -> Result<ChapterCutoverPlan, TransitionPlanError> {
    let identity = crate::CommandActivityIdentity::new(input.command_id);
    let transaction_id = identity.transaction_id();
    let fact = |ordinal, value| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: identity.correlation_id(),
        caused_by_activity_id: None,
        command_id: Some(input.command_id),
        host_request_id: None,
        actor: ActivityActor::Migration,
        origin: ActivityOrigin::Migration,
        subject: ActivitySubject::Global,
        episode_id: None,
        fact: value,
    };
    let accepted = input.disposition == RequestDisposition::Accepted;
    if accepted && input.committed_revision.value <= input.current_revision.value {
        return Err(TransitionPlanError::DispositionRequiresTransition);
    }
    let head = fact(
        0,
        ActivityFact::RequestDisposition {
            disposition: input.disposition,
        },
    );
    let facts = if accepted {
        NonEmptyActivityFacts::from_head_and_tail(
            head,
            vec![
                fact(
                    1,
                    ActivityFact::DomainTransition {
                        kind: DomainTransitionKind::Chapter(ChapterTransition::SelectionChanged),
                        previous_revision: input.current_revision,
                        committed_revision: input.committed_revision,
                    },
                ),
                fact(
                    2,
                    ActivityFact::AuthorityCutover {
                        domain: ActivityDomain::Chapter,
                    },
                ),
            ],
        )
    } else {
        NonEmptyActivityFacts::new(head)
    };
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        if accepted {
            ChapterCutoverMutation::Apply
        } else {
            ChapterCutoverMutation::None
        },
        facts,
        Vec::new(),
        Vec::new(),
    )
}
