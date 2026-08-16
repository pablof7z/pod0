use pod0_domain::{CommandId, EpisodeId, StateRevision};

use crate::{
    ActivityActor, ActivityDomain, ActivityFact, ActivityFactDraft, ActivityOrigin,
    ActivitySubject, AuthorizedInternalCommand, DomainTransitionKind, DurableExternalEffectRequest,
    DurableInternalCommandRequest, InternalCommandKind, LifecycleTransition, NonEmptyActivityFacts,
    RequestDisposition, TransitionPlan, TransitionPlanError, WorkflowOpportunity,
    WorkflowReconcileIntent, WorkflowReconcilePlan,
};

pub struct WorkflowReconcileActivityInput {
    pub command_id: CommandId,
    pub current_revision: StateRevision,
    pub opportunity: WorkflowOpportunity,
    pub plan: WorkflowReconcilePlan,
    pub disposition: RequestDisposition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkflowReconcileMutation {
    pub opportunity: WorkflowOpportunity,
    pub next_episode_offset: Option<u32>,
}

pub type WorkflowReconcileTransitionPlan = TransitionPlan<
    WorkflowReconcileMutation,
    DurableExternalEffectRequest,
    DurableInternalCommandRequest,
>;

pub fn plan_workflow_reconcile_activity(
    input: WorkflowReconcileActivityInput,
) -> Result<WorkflowReconcileTransitionPlan, TransitionPlanError> {
    let max_intents = crate::MAX_WORKFLOW_RECONCILE_EPISODES_PER_PAGE
        .saturating_mul(3)
        .saturating_add(1);
    if input.plan.intents.len() > max_intents
        || input.opportunity.observed_at.value < 0
        || input.opportunity.capability_snapshot_id.into_bytes() == [0; 32]
    {
        return Err(TransitionPlanError::InvalidEffectAuthorization);
    }
    let accepted = input.disposition == RequestDisposition::Accepted;
    if !accepted && (!input.plan.intents.is_empty() || input.plan.next_episode_offset.is_some()) {
        return Err(TransitionPlanError::InvalidEffectAuthorization);
    }
    let committed_revision = StateRevision::new(
        input
            .current_revision
            .value
            .checked_add(1)
            .ok_or(TransitionPlanError::RevisionExhausted)?,
    );
    let identity = crate::CommandActivityIdentity::new(input.command_id);
    let transaction_id = identity.transaction_id();
    let draft = |ordinal, subject, episode_id, fact| ActivityFactDraft {
        activity_id: identity.fact_id_wide(ordinal),
        transaction_id,
        correlation_id: identity.correlation_id(),
        caused_by_activity_id: None,
        command_id: Some(input.command_id),
        host_request_id: None,
        actor: ActivityActor::System,
        origin: ActivityOrigin::AutomaticPolicy,
        subject,
        episode_id,
        fact,
    };
    let mut tail = accepted
        .then(|| {
            draft(
                1,
                ActivitySubject::Global,
                None,
                ActivityFact::DomainTransition {
                    kind: DomainTransitionKind::Lifecycle(
                        LifecycleTransition::WorkflowReconciliationPlanned,
                    ),
                    previous_revision: input.current_revision,
                    committed_revision,
                },
            )
        })
        .into_iter()
        .collect::<Vec<_>>();
    let mut commands = Vec::new();
    for (index, intent) in input.plan.intents.into_iter().enumerate() {
        authorize_intent(&identity, index, intent, &mut tail, &mut commands);
    }
    if let Some(episode_offset) = input.plan.next_episode_offset {
        let index = commands.len();
        authorize(
            &identity,
            index,
            ActivityDomain::Lifecycle,
            ActivitySubject::Global,
            None,
            InternalCommandKind::ContinueWorkflowReconciliation {
                opportunity: input.opportunity,
                episode_offset,
            },
            &mut tail,
            &mut commands,
        );
    }
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        WorkflowReconcileMutation {
            opportunity: input.opportunity,
            next_episode_offset: input.plan.next_episode_offset,
        },
        NonEmptyActivityFacts::from_head_and_tail(
            draft(
                0,
                ActivitySubject::Global,
                None,
                ActivityFact::RequestDisposition {
                    disposition: input.disposition,
                },
            ),
            tail,
        ),
        Vec::new(),
        commands,
    )
}

fn authorize_intent(
    identity: &crate::CommandActivityIdentity,
    index: usize,
    intent: WorkflowReconcileIntent,
    facts: &mut Vec<ActivityFactDraft>,
    commands: &mut Vec<AuthorizedInternalCommand<DurableInternalCommandRequest>>,
) {
    let (target, subject, episode_id, kind) = match intent {
        WorkflowReconcileIntent::EnsurePublisherChapters { episode_id } => (
            ActivityDomain::Chapter,
            ActivitySubject::Episode { episode_id },
            Some(episode_id),
            InternalCommandKind::EnsurePublisherChapters,
        ),
        WorkflowReconcileIntent::EnsureTranscript {
            episode_id,
            origin,
            configuration,
        } => (
            ActivityDomain::Transcript,
            ActivitySubject::Episode { episode_id },
            Some(episode_id),
            InternalCommandKind::EnsureTranscriptWorkflow {
                origin,
                configuration,
            },
        ),
        WorkflowReconcileIntent::EnsureModelChapters {
            episode_id,
            configured_model,
        } => (
            ActivityDomain::Chapter,
            ActivitySubject::Episode { episode_id },
            Some(episode_id),
            InternalCommandKind::EnsureModelChapters { configured_model },
        ),
        WorkflowReconcileIntent::ReconcileScheduledRuns => (
            ActivityDomain::ScheduledAgent,
            ActivitySubject::Global,
            None,
            InternalCommandKind::ReconcileScheduledRuns,
        ),
    };
    authorize(
        identity, index, target, subject, episode_id, kind, facts, commands,
    );
}

fn authorize(
    identity: &crate::CommandActivityIdentity,
    index: usize,
    target: ActivityDomain,
    subject: ActivitySubject,
    episode_id: Option<EpisodeId>,
    kind: InternalCommandKind,
    facts: &mut Vec<ActivityFactDraft>,
    commands: &mut Vec<AuthorizedInternalCommand<DurableInternalCommandRequest>>,
) {
    let ordinal = u8::try_from(index).expect("bounded reconciliation command count");
    let internal_command_id = identity.internal_command_id(ordinal);
    let fact_index = facts.len().saturating_add(1);
    facts.push(ActivityFactDraft {
        activity_id: identity.fact_id_wide(u32::try_from(fact_index).unwrap_or(u32::MAX)),
        transaction_id: identity.transaction_id(),
        correlation_id: identity.correlation_id(),
        caused_by_activity_id: None,
        command_id: None,
        host_request_id: None,
        actor: ActivityActor::System,
        origin: ActivityOrigin::InternalCommand,
        subject,
        episode_id,
        fact: ActivityFact::InternalCommandAuthorized {
            internal_command_id,
            target,
        },
    });
    commands.push(AuthorizedInternalCommand {
        internal_command_id,
        authorizing_fact_index: fact_index,
        command: DurableInternalCommandRequest {
            kind,
            target,
            subject,
            episode_id,
        },
    });
}
