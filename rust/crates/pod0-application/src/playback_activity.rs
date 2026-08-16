use pod0_domain::{CommandId, EpisodeId, StateRevision};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    AuthorizedExternalEffect, AuthorizedInternalCommand, DomainTransitionKind,
    DurableEffectExecution, DurableExternalEffectRequest, DurableInternalCommandRequest,
    DurablePlaybackEffectRequest, ExternalEffectKind, NonEmptyActivityFacts, PlaybackTransition,
    RequestDisposition, TransitionPlan, TransitionPlanError,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaybackActivityInput {
    pub command_id: CommandId,
    pub episode_id: Option<EpisodeId>,
    pub current_revision: StateRevision,
    pub legacy_command_revision: Option<StateRevision>,
    pub transition: PlaybackTransition,
    pub checkpoint_position_milliseconds: Option<u64>,
    pub internal_command: Option<DurableInternalCommandRequest>,
    pub effects: Vec<DurablePlaybackEffectRequest>,
    pub superseded_effects: Vec<crate::CancellationEffectTarget>,
}

pub type PlaybackActivityPlan =
    TransitionPlan<(), DurableExternalEffectRequest, DurableInternalCommandRequest>;

pub fn plan_playback_activity(
    input: PlaybackActivityInput,
) -> Result<PlaybackActivityPlan, TransitionPlanError> {
    if input
        .effects
        .len()
        .saturating_add(input.superseded_effects.len())
        > 200
        || input
            .effects
            .iter()
            .any(|effect| effect.command_id != input.command_id)
    {
        return Err(TransitionPlanError::InvalidEffectAuthorization);
    }
    let identity = crate::CommandActivityIdentity::new(input.command_id);
    let transaction_id = identity.transaction_id();
    let subject = input
        .episode_id
        .map_or(ActivitySubject::Global, |episode_id| {
            ActivitySubject::Episode { episode_id }
        });
    let disposition = if input.legacy_command_revision.is_some() {
        RequestDisposition::Duplicate
    } else {
        RequestDisposition::Accepted
    };
    let base = |ordinal, fact| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: identity.correlation_id(),
        caused_by_activity_id: None,
        command_id: Some(input.command_id),
        host_request_id: None,
        // The current command envelope has no trusted caller provenance. Do
        // not invent a user/agent attribution; the playback machine is the
        // durable actor until command provenance becomes typed at ingress.
        actor: ActivityActor::System,
        origin: ActivityOrigin::Playback,
        subject,
        episode_id: input.episode_id,
        fact,
    };
    let head = base(0, ActivityFact::RequestDisposition { disposition });
    let mut internal_commands = Vec::new();
    let mut external_effects = Vec::new();
    let facts = if disposition == RequestDisposition::Accepted {
        let committed_revision = StateRevision::new(
            input
                .current_revision
                .value
                .checked_add(1)
                .ok_or(TransitionPlanError::RevisionExhausted)?,
        );
        let mut tail = vec![base(
            1,
            ActivityFact::DomainTransition {
                kind: DomainTransitionKind::Playback(input.transition),
                previous_revision: input.current_revision,
                committed_revision,
            },
        )];
        if let Some(position_milliseconds) = input.checkpoint_position_milliseconds {
            tail.push(base(
                2,
                ActivityFact::PlaybackCheckpoint {
                    position_milliseconds,
                },
            ));
        }
        if let Some(command) = input.internal_command {
            let internal_command_id = identity.internal_command_id(0);
            let fact_ordinal = u8::try_from(tail.len() + 1)
                .map_err(|_| TransitionPlanError::InvalidEffectAuthorization)?;
            let fact_index = tail.len() + 1;
            tail.push(base(
                fact_ordinal,
                ActivityFact::InternalCommandAuthorized {
                    internal_command_id,
                    target: command.target,
                },
            ));
            internal_commands.push(AuthorizedInternalCommand {
                internal_command_id,
                authorizing_fact_index: fact_index,
                command,
            });
        }
        let first_effect_ordinal = tail.len() + 1;
        for (index, request) in input.effects.into_iter().enumerate() {
            let effect_ordinal =
                u8::try_from(index).map_err(|_| TransitionPlanError::InvalidEffectAuthorization)?;
            let intent_id = identity.effect_intent_id(effect_ordinal);
            let fact_index = first_effect_ordinal + index;
            let fact_ordinal = u8::try_from(fact_index)
                .map_err(|_| TransitionPlanError::InvalidEffectAuthorization)?;
            let episode_id = request.episode_id();
            let effect_subject = episode_id.map_or(ActivitySubject::Global, |episode_id| {
                ActivitySubject::Episode { episode_id }
            });
            let mut fact = base(
                fact_ordinal,
                ActivityFact::EffectAuthorized {
                    intent_id,
                    kind: ExternalEffectKind::Playback,
                },
            );
            fact.subject = effect_subject;
            fact.episode_id = episode_id;
            fact.host_request_id = Some(request.request_id);
            tail.push(fact);
            external_effects.push(AuthorizedExternalEffect {
                intent_id,
                authorizing_fact_index: fact_index,
                request: DurableExternalEffectRequest {
                    kind: ExternalEffectKind::Playback,
                    subject: effect_subject,
                    episode_id,
                    not_before: None,
                    deadline_at: request.deadline_at,
                    execution: DurableEffectExecution::Playback { request },
                },
            });
        }
        let first_superseded_ordinal = tail.len() + 1;
        for (index, target) in input.superseded_effects.into_iter().enumerate() {
            let fact_ordinal = u8::try_from(first_superseded_ordinal + index)
                .map_err(|_| TransitionPlanError::InvalidEffectAuthorization)?;
            let mut fact = base(
                fact_ordinal,
                ActivityFact::RecoveryTransition {
                    outcome: crate::EffectOutcome::Superseded,
                },
            );
            fact.subject = target.subject;
            fact.episode_id = target.episode_id;
            fact.host_request_id = Some(target.host_request_id);
            tail.push(fact);
        }
        NonEmptyActivityFacts::from_head_and_tail(head, tail)
    } else {
        NonEmptyActivityFacts::new(head)
    };
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        (),
        facts,
        external_effects,
        internal_commands,
    )
}
