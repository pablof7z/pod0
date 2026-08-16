use pod0_domain::{ActivityCorrelationId, ActivityId, CommandId, InternalCommandId, StateRevision};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    AuthorizedExternalEffect, AuthorizedInternalCommand, DomainTransitionKind,
    DurableExternalEffectRequest, DurableInternalCommandRequest, InternalCommandActivityIdentity,
    NonEmptyActivityFacts, RequestDisposition, TransitionPlan, TransitionPlanError,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InternalCommandOwnerActivityInput {
    pub internal_command_id: InternalCommandId,
    pub authorizing_activity_id: ActivityId,
    pub correlation_id: ActivityCorrelationId,
    pub command_id: CommandId,
    pub subject: ActivitySubject,
    pub episode_id: Option<pod0_domain::EpisodeId>,
    pub current_revision: StateRevision,
    pub committed_revision: StateRevision,
    pub disposition: RequestDisposition,
    pub transitions: Vec<(ActivitySubject, DomainTransitionKind)>,
    pub effects: Vec<DurableExternalEffectRequest>,
    pub internal_commands: Vec<DurableInternalCommandRequest>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApplyInternalCommandOwner {
    pub changes_state: bool,
}

pub type InternalCommandOwnerPlan = TransitionPlan<
    ApplyInternalCommandOwner,
    DurableExternalEffectRequest,
    DurableInternalCommandRequest,
>;

pub fn plan_internal_command_owner_activity(
    input: InternalCommandOwnerActivityInput,
) -> Result<InternalCommandOwnerPlan, TransitionPlanError> {
    let changes_state = input.committed_revision != input.current_revision;
    let accepted = input.disposition == RequestDisposition::Accepted;
    if changes_state != accepted
        || (!accepted
            && (!input.transitions.is_empty()
                || !input.effects.is_empty()
                || !input.internal_commands.is_empty()))
        || (accepted && input.committed_revision.value <= input.current_revision.value)
    {
        return Err(TransitionPlanError::DispositionRequiresTransition);
    }
    let identity = InternalCommandActivityIdentity::new(input.internal_command_id);
    let transaction_id = identity.transaction_id();
    let base = |ordinal, subject, episode_id, fact| ActivityFactDraft {
        activity_id: identity.fact_id_wide(ordinal),
        transaction_id,
        correlation_id: input.correlation_id,
        caused_by_activity_id: Some(input.authorizing_activity_id),
        command_id: Some(input.command_id),
        host_request_id: None,
        actor: ActivityActor::System,
        origin: ActivityOrigin::InternalCommand,
        subject,
        episode_id,
        fact,
    };
    let mut tail = Vec::new();
    for (subject, transition) in input.transitions {
        tail.push(base(
            u32::try_from(tail.len() + 1).unwrap_or(u32::MAX),
            subject,
            episode_id(subject),
            ActivityFact::DomainTransition {
                kind: transition,
                previous_revision: input.current_revision,
                committed_revision: input.committed_revision,
            },
        ));
    }
    let mut effects = Vec::with_capacity(input.effects.len());
    for (ordinal, request) in input.effects.into_iter().enumerate() {
        let intent_id = identity.effect_intent_id(
            u8::try_from(ordinal).map_err(|_| TransitionPlanError::InvalidEffectAuthorization)?,
        );
        let fact_index = tail.len() + 1;
        tail.push(base(
            u32::try_from(fact_index).unwrap_or(u32::MAX),
            request.subject,
            request.episode_id,
            ActivityFact::EffectAuthorized {
                intent_id,
                kind: request.kind,
            },
        ));
        effects.push(AuthorizedExternalEffect {
            intent_id,
            authorizing_fact_index: fact_index,
            request,
        });
    }
    let mut commands = Vec::with_capacity(input.internal_commands.len());
    for (ordinal, command) in input.internal_commands.into_iter().enumerate() {
        let command_id = identity.internal_command_id(
            u8::try_from(ordinal).map_err(|_| TransitionPlanError::TooManyInternalCommands)?,
        );
        let fact_index = tail.len() + 1;
        tail.push(base(
            u32::try_from(fact_index).unwrap_or(u32::MAX),
            command.subject,
            command.episode_id,
            ActivityFact::InternalCommandAuthorized {
                internal_command_id: command_id,
                target: command.target,
            },
        ));
        commands.push(AuthorizedInternalCommand {
            internal_command_id: command_id,
            authorizing_fact_index: fact_index,
            command,
        });
    }
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        ApplyInternalCommandOwner { changes_state },
        NonEmptyActivityFacts::from_head_and_tail(
            base(
                0,
                input.subject,
                input.episode_id,
                ActivityFact::RequestDisposition {
                    disposition: input.disposition,
                },
            ),
            tail,
        ),
        effects,
        commands,
    )
}

fn episode_id(subject: ActivitySubject) -> Option<pod0_domain::EpisodeId> {
    match subject {
        ActivitySubject::Episode { episode_id } => Some(episode_id),
        _ => None,
    }
}
