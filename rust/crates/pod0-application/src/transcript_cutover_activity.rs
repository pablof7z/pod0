use pod0_domain::{CommandId, StateRevision};

use crate::{
    ActivityActor, ActivityDomain, ActivityFact, ActivityFactDraft, ActivityOrigin,
    ActivitySubject, DomainTransitionKind, DurableExternalEffectRequest,
    DurableInternalCommandRequest, NonEmptyActivityFacts, RequestDisposition, TranscriptTransition,
    TransitionPlan, TransitionPlanError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TranscriptCutoverActivityInput {
    pub command_id: CommandId,
    pub current_revision: StateRevision,
    pub committed_revision: StateRevision,
    pub disposition: RequestDisposition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TranscriptCutoverMutation {
    Apply,
    None,
}

pub type TranscriptCutoverPlan = TransitionPlan<
    TranscriptCutoverMutation,
    DurableExternalEffectRequest,
    DurableInternalCommandRequest,
>;

pub fn plan_transcript_cutover(
    input: TranscriptCutoverActivityInput,
) -> Result<TranscriptCutoverPlan, TransitionPlanError> {
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
                        kind: DomainTransitionKind::Transcript(
                            TranscriptTransition::SelectionChanged,
                        ),
                        previous_revision: input.current_revision,
                        committed_revision: input.committed_revision,
                    },
                ),
                fact(
                    2,
                    ActivityFact::AuthorityCutover {
                        domain: ActivityDomain::Transcript,
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
            TranscriptCutoverMutation::Apply
        } else {
            TranscriptCutoverMutation::None
        },
        facts,
        Vec::new(),
        Vec::new(),
    )
}
