use pod0_domain::{CommandId, StateRevision};

use crate::{
    ActivityActor, ActivityDomain, ActivityFact, ActivityFactDraft, ActivityOrigin,
    ActivitySubject, DomainTransitionKind, DurableExternalEffectRequest,
    DurableInternalCommandRequest, LifecycleTransition, NonEmptyActivityFacts, RequestDisposition,
    TransitionPlan, TransitionPlanError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkflowConfigurationActivityKind {
    ImportAuthority,
    Set,
    ObserveCapabilities,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkflowConfigurationActivityInput {
    pub command_id: CommandId,
    pub current_revision: StateRevision,
    pub committed_revision: StateRevision,
    pub disposition: RequestDisposition,
    pub kind: WorkflowConfigurationActivityKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkflowConfigurationMutation {
    Apply,
    None,
}

pub type WorkflowConfigurationActivityPlan = TransitionPlan<
    WorkflowConfigurationMutation,
    DurableExternalEffectRequest,
    DurableInternalCommandRequest,
>;

pub fn plan_workflow_configuration_activity(
    input: WorkflowConfigurationActivityInput,
) -> Result<WorkflowConfigurationActivityPlan, TransitionPlanError> {
    let accepted = input.disposition == RequestDisposition::Accepted;
    if accepted && input.committed_revision.value <= input.current_revision.value {
        return Err(TransitionPlanError::DispositionRequiresTransition);
    }
    let identity = crate::CommandActivityIdentity::new(input.command_id);
    let transaction_id = identity.transaction_id();
    let actor = if input.kind == WorkflowConfigurationActivityKind::ImportAuthority {
        ActivityActor::Migration
    } else {
        ActivityActor::User
    };
    let origin = match input.kind {
        WorkflowConfigurationActivityKind::ImportAuthority => ActivityOrigin::Migration,
        WorkflowConfigurationActivityKind::Set => ActivityOrigin::UserInterface,
        WorkflowConfigurationActivityKind::ObserveCapabilities => ActivityOrigin::HostObservation,
    };
    let fact = |ordinal, fact| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: identity.correlation_id(),
        caused_by_activity_id: None,
        command_id: Some(input.command_id),
        host_request_id: None,
        actor,
        origin,
        subject: ActivitySubject::Global,
        episode_id: None,
        fact,
    };
    let head = fact(
        0,
        ActivityFact::RequestDisposition {
            disposition: input.disposition,
        },
    );
    let facts = if accepted {
        let transition = match input.kind {
            WorkflowConfigurationActivityKind::ImportAuthority
            | WorkflowConfigurationActivityKind::Set => {
                LifecycleTransition::WorkflowConfigurationChanged
            }
            WorkflowConfigurationActivityKind::ObserveCapabilities => {
                LifecycleTransition::WorkflowCapabilitiesObserved
            }
        };
        let mut tail = vec![fact(
            1,
            ActivityFact::DomainTransition {
                kind: DomainTransitionKind::Lifecycle(transition),
                previous_revision: input.current_revision,
                committed_revision: input.committed_revision,
            },
        )];
        if input.kind == WorkflowConfigurationActivityKind::ImportAuthority {
            tail.push(fact(
                2,
                ActivityFact::AuthorityCutover {
                    domain: ActivityDomain::Lifecycle,
                },
            ));
        }
        NonEmptyActivityFacts::from_head_and_tail(head, tail)
    } else {
        NonEmptyActivityFacts::new(head)
    };
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        if accepted {
            WorkflowConfigurationMutation::Apply
        } else {
            WorkflowConfigurationMutation::None
        },
        facts,
        Vec::new(),
        Vec::new(),
    )
}
