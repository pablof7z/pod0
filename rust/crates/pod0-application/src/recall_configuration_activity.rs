use pod0_domain::{CommandId, StateRevision};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    DomainTransitionKind, DurableExternalEffectRequest, DurableInternalCommandRequest,
    NonEmptyActivityFacts, RecallKnowledgeTransition, RequestDisposition, TransitionPlan,
    TransitionPlanError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecallConfigurationActivityInput {
    pub command_id: CommandId,
    pub current_revision: StateRevision,
    pub committed_revision: StateRevision,
    pub disposition: RequestDisposition,
    pub migration: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecallConfigurationMutation {
    Apply,
    None,
}

pub type RecallConfigurationActivityPlan = TransitionPlan<
    RecallConfigurationMutation,
    DurableExternalEffectRequest,
    DurableInternalCommandRequest,
>;

pub fn plan_recall_configuration_activity(
    input: RecallConfigurationActivityInput,
) -> Result<RecallConfigurationActivityPlan, TransitionPlanError> {
    let identity = crate::CommandActivityIdentity::new(input.command_id);
    let transaction_id = identity.transaction_id();
    let base = |ordinal, fact| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: identity.correlation_id(),
        caused_by_activity_id: None,
        command_id: Some(input.command_id),
        host_request_id: None,
        actor: if input.migration {
            ActivityActor::Migration
        } else {
            ActivityActor::User
        },
        origin: if input.migration {
            ActivityOrigin::Migration
        } else {
            ActivityOrigin::UserInterface
        },
        subject: ActivitySubject::Global,
        episode_id: None,
        fact,
    };
    let accepted = input.disposition == RequestDisposition::Accepted;
    if accepted && input.committed_revision.value <= input.current_revision.value {
        return Err(TransitionPlanError::DispositionRequiresTransition);
    }
    let head = base(
        0,
        ActivityFact::RequestDisposition {
            disposition: input.disposition,
        },
    );
    let facts = if accepted {
        NonEmptyActivityFacts::from_head_and_tail(
            head,
            vec![base(
                1,
                ActivityFact::DomainTransition {
                    kind: DomainTransitionKind::RecallKnowledge(
                        RecallKnowledgeTransition::ConfigurationChanged,
                    ),
                    previous_revision: input.current_revision,
                    committed_revision: input.committed_revision,
                },
            )],
        )
    } else {
        NonEmptyActivityFacts::new(head)
    };
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        if accepted {
            RecallConfigurationMutation::Apply
        } else {
            RecallConfigurationMutation::None
        },
        facts,
        Vec::new(),
        Vec::new(),
    )
}
