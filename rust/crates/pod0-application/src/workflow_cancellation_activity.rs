use pod0_domain::{CommandId, EpisodeId, StateRevision};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    CancellationEffectTarget, CommandActivityIdentity, DomainTransitionKind,
    DurableExternalEffectRequest, DurableInternalCommandRequest, ExternalEffectKind,
    NonEmptyActivityFacts, RequestDisposition, TransitionPlan, TransitionPlanError,
    prepare_cancellation_authorization,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkflowCancellationActivityInput {
    pub command_id: CommandId,
    pub episode_id: EpisodeId,
    pub subject: ActivitySubject,
    pub current_revision: StateRevision,
    pub transition: DomainTransitionKind,
    pub target: Option<CancellationEffectTarget>,
}

pub type WorkflowCancellationActivityPlan =
    TransitionPlan<(), DurableExternalEffectRequest, DurableInternalCommandRequest>;

pub fn plan_workflow_cancellation_activity(
    input: WorkflowCancellationActivityInput,
) -> Result<WorkflowCancellationActivityPlan, TransitionPlanError> {
    let identity = CommandActivityIdentity::new(input.command_id);
    let transaction_id = identity.transaction_id();
    let committed_revision = StateRevision::new(
        input
            .current_revision
            .value
            .checked_add(1)
            .ok_or(TransitionPlanError::RevisionExhausted)?,
    );
    let fact = |ordinal, host_request_id, value| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: identity.correlation_id(),
        caused_by_activity_id: None,
        command_id: Some(input.command_id),
        host_request_id,
        actor: ActivityActor::User,
        origin: ActivityOrigin::UserInterface,
        subject: input.subject,
        episode_id: Some(input.episode_id),
        fact: value,
    };
    let mut tail = vec![fact(
        1,
        None,
        ActivityFact::DomainTransition {
            kind: input.transition,
            previous_revision: input.current_revision,
            committed_revision,
        },
    )];
    let effects = input.target.map_or_else(Vec::new, |target| {
        tail.push(fact(
            2,
            Some(target.host_request_id),
            ActivityFact::RecoveryTransition {
                outcome: crate::EffectOutcome::Superseded,
            },
        ));
        let authorization = prepare_cancellation_authorization(
            input.command_id,
            input.current_revision,
            0,
            3,
            target,
        );
        tail.push(fact(
            3,
            Some(authorization.request_id),
            ActivityFact::EffectAuthorized {
                intent_id: authorization.intent_id,
                kind: ExternalEffectKind::Cancellation,
            },
        ));
        vec![authorization.effect]
    });
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        (),
        NonEmptyActivityFacts::from_head_and_tail(
            fact(
                0,
                None,
                ActivityFact::RequestDisposition {
                    disposition: RequestDisposition::Accepted,
                },
            ),
            tail,
        ),
        effects,
        Vec::new(),
    )
}
