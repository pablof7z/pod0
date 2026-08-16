use pod0_domain::{
    ActivityCorrelationId, ActivityId, CommandId, EffectAttemptId, EffectIntentId, EpisodeId,
    HostRequestId, StateRevision,
};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    AuthorizedExternalEffect, CancellationEffectTarget, DomainTransitionKind,
    DurableEffectExecution, DurableExternalEffectRequest, DurableInternalCommandRequest,
    DurablePlaybackEffectRequest, EffectObservationActivityIdentity, EffectOutcome,
    ExternalEffectKind, NonEmptyActivityFacts, PlaybackTransition, RequestDisposition,
    TransitionPlan, TransitionPlanError,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaybackObservationActivityInput {
    pub identity_attempt_id: EffectAttemptId,
    pub effect_attempt_id: EffectAttemptId,
    pub request_id: HostRequestId,
    pub command_id: CommandId,
    pub episode_id: Option<EpisodeId>,
    pub current_revision: StateRevision,
    pub intent_id: EffectIntentId,
    pub authorizing_activity_id: ActivityId,
    pub correlation_id: ActivityCorrelationId,
    pub outcome: EffectOutcome,
    pub reaction: Option<PlaybackObservationReactionActivityInput>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaybackObservationReactionActivityInput {
    pub command_id: CommandId,
    pub transition: PlaybackTransition,
    pub checkpoint_position_milliseconds: Option<u64>,
    pub effects: Vec<DurablePlaybackEffectRequest>,
    pub superseded_effects: Vec<CancellationEffectTarget>,
}

pub type PlaybackObservationActivityPlan =
    TransitionPlan<(), DurableExternalEffectRequest, DurableInternalCommandRequest>;

pub fn plan_playback_observation(
    input: PlaybackObservationActivityInput,
) -> Result<PlaybackObservationActivityPlan, TransitionPlanError> {
    if input.reaction.as_ref().is_some_and(|reaction| {
        reaction
            .effects
            .len()
            .saturating_add(reaction.superseded_effects.len())
            > 200
            || reaction
                .effects
                .iter()
                .any(|effect| effect.command_id != reaction.command_id)
    }) {
        return Err(TransitionPlanError::InvalidEffectAuthorization);
    }
    let identity = EffectObservationActivityIdentity::new(input.identity_attempt_id);
    let transaction_id = identity.transaction_id();
    let subject = input
        .episode_id
        .map_or(ActivitySubject::Global, |episode_id| {
            ActivitySubject::Episode { episode_id }
        });
    let base = |ordinal, command_id, fact| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: input.correlation_id,
        caused_by_activity_id: Some(input.authorizing_activity_id),
        command_id: Some(command_id),
        host_request_id: Some(input.request_id),
        actor: ActivityActor::System,
        origin: ActivityOrigin::HostObservation,
        subject,
        episode_id: input.episode_id,
        fact,
    };
    let mut tail = vec![base(
        1,
        input.command_id,
        ActivityFact::EffectObserved {
            intent_id: input.intent_id,
            attempt_id: input.effect_attempt_id,
            outcome: input.outcome,
        },
    )];
    let mut effects = Vec::new();
    if let Some(reaction) = input.reaction {
        let committed_revision = StateRevision::new(
            input
                .current_revision
                .value
                .checked_add(1)
                .ok_or(TransitionPlanError::RevisionExhausted)?,
        );
        tail.push(base(
            2,
            reaction.command_id,
            ActivityFact::DomainTransition {
                kind: DomainTransitionKind::Playback(reaction.transition),
                previous_revision: input.current_revision,
                committed_revision,
            },
        ));
        if let Some(position_milliseconds) = reaction.checkpoint_position_milliseconds {
            let ordinal = u8::try_from(tail.len() + 1)
                .map_err(|_| TransitionPlanError::InvalidEffectAuthorization)?;
            tail.push(base(
                ordinal,
                reaction.command_id,
                ActivityFact::PlaybackCheckpoint {
                    position_milliseconds,
                },
            ));
        }
        for request in reaction.effects {
            let fact_index = tail.len() + 1;
            let ordinal = u8::try_from(fact_index)
                .map_err(|_| TransitionPlanError::InvalidEffectAuthorization)?;
            let effect_ordinal = u8::try_from(effects.len())
                .map_err(|_| TransitionPlanError::InvalidEffectAuthorization)?;
            let intent_id = identity.effect_intent_id(effect_ordinal);
            let episode_id = request.episode_id();
            let effect_subject = episode_id.map_or(ActivitySubject::Global, |episode_id| {
                ActivitySubject::Episode { episode_id }
            });
            let mut fact = base(
                ordinal,
                reaction.command_id,
                ActivityFact::EffectAuthorized {
                    intent_id,
                    kind: ExternalEffectKind::Playback,
                },
            );
            fact.subject = effect_subject;
            fact.episode_id = episode_id;
            fact.host_request_id = Some(request.request_id);
            tail.push(fact);
            effects.push(AuthorizedExternalEffect {
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
        for target in reaction.superseded_effects {
            let ordinal = u8::try_from(tail.len() + 1)
                .map_err(|_| TransitionPlanError::InvalidEffectAuthorization)?;
            let mut fact = base(
                ordinal,
                reaction.command_id,
                ActivityFact::RecoveryTransition {
                    outcome: EffectOutcome::Superseded,
                },
            );
            fact.subject = target.subject;
            fact.episode_id = target.episode_id;
            fact.host_request_id = Some(target.host_request_id);
            tail.push(fact);
        }
    }
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        (),
        NonEmptyActivityFacts::from_head_and_tail(
            base(
                0,
                input.command_id,
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

#[must_use]
pub fn playback_observation_identity(
    attempt_id: EffectAttemptId,
    sequence_number: u64,
) -> EffectAttemptId {
    use sha2::{Digest as _, Sha256};
    let mut hash = Sha256::new();
    hash.update(b"pod0/playback-observation/v1");
    hash.update(attempt_id.into_bytes());
    hash.update(sequence_number.to_be_bytes());
    let digest: [u8; 32] = hash.finalize().into();
    EffectAttemptId::from_bytes(digest[..16].try_into().expect("fixed digest prefix"))
}
