use pod0_domain::{CommandId, StateRevision};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    AuthorizedExternalEffect, CancellationEffectTarget, DomainTransitionKind,
    DurableEffectExecution, DurableExternalEffectRequest, DurableInternalCommandRequest,
    DurablePlaybackEffectRequest, EffectOutcome, ExternalEffectKind, LibraryFeedTransition,
    LifecycleTransition, NonEmptyActivityFacts, RequestDisposition, TransitionPlan,
    TransitionPlanError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListeningResetMutation {
    Reset,
    Duplicate { committed_revision: StateRevision },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListeningResetActivityInput {
    pub command_id: CommandId,
    pub current_revision: StateRevision,
    pub legacy_command_revision: Option<StateRevision>,
    pub effects: Vec<DurablePlaybackEffectRequest>,
    pub superseded_effects: Vec<CancellationEffectTarget>,
}

pub type ListeningResetActivityPlan = TransitionPlan<
    ListeningResetMutation,
    DurableExternalEffectRequest,
    DurableInternalCommandRequest,
>;

pub fn plan_listening_reset(
    input: ListeningResetActivityInput,
) -> Result<ListeningResetActivityPlan, TransitionPlanError> {
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
        actor: ActivityActor::User,
        origin: ActivityOrigin::UserInterface,
        subject: ActivitySubject::Global,
        episode_id: None,
        fact,
    };
    let head = base(0, ActivityFact::RequestDisposition { disposition });
    if disposition == RequestDisposition::Duplicate {
        return TransitionPlan::new(
            transaction_id,
            input.current_revision,
            ListeningResetMutation::Duplicate {
                committed_revision: input
                    .legacy_command_revision
                    .expect("duplicate reset has a committed revision"),
            },
            NonEmptyActivityFacts::new(head),
            Vec::new(),
            Vec::new(),
        );
    }
    let committed_revision = StateRevision::new(
        input
            .current_revision
            .value
            .checked_add(1)
            .ok_or(TransitionPlanError::RevisionExhausted)?,
    );
    let mut tail = vec![
        base(
            1,
            ActivityFact::DomainTransition {
                kind: DomainTransitionKind::Lifecycle(LifecycleTransition::UserDataErasureChanged),
                previous_revision: input.current_revision,
                committed_revision,
            },
        ),
        base(
            2,
            ActivityFact::DomainTransition {
                kind: DomainTransitionKind::LibraryFeed(LibraryFeedTransition::ListeningDataReset),
                previous_revision: input.current_revision,
                committed_revision,
            },
        ),
    ];
    let first_effect_index = tail.len() + 1;
    let mut effects = Vec::with_capacity(input.effects.len());
    for (index, request) in input.effects.into_iter().enumerate() {
        let episode_id = request.episode_id();
        let subject = episode_id.map_or(ActivitySubject::Global, |episode_id| {
            ActivitySubject::Episode { episode_id }
        });
        let intent_id = identity.effect_intent_id(
            u8::try_from(index).map_err(|_| TransitionPlanError::InvalidEffectAuthorization)?,
        );
        let fact_index = first_effect_index + index;
        let mut fact = base(
            u8::try_from(fact_index)
                .map_err(|_| TransitionPlanError::InvalidEffectAuthorization)?,
            ActivityFact::EffectAuthorized {
                intent_id,
                kind: ExternalEffectKind::Playback,
            },
        );
        fact.subject = subject;
        fact.episode_id = episode_id;
        fact.host_request_id = Some(request.request_id);
        tail.push(fact);
        effects.push(AuthorizedExternalEffect {
            intent_id,
            authorizing_fact_index: fact_index,
            request: DurableExternalEffectRequest {
                kind: ExternalEffectKind::Playback,
                subject,
                episode_id,
                not_before: None,
                deadline_at: request.deadline_at,
                execution: DurableEffectExecution::Playback { request },
            },
        });
    }
    let first_superseded_index = tail.len() + 1;
    for (index, target) in input.superseded_effects.into_iter().enumerate() {
        let mut fact = base(
            u8::try_from(first_superseded_index + index)
                .map_err(|_| TransitionPlanError::InvalidEffectAuthorization)?,
            ActivityFact::RecoveryTransition {
                outcome: EffectOutcome::Superseded,
            },
        );
        fact.subject = target.subject;
        fact.episode_id = target.episode_id;
        fact.host_request_id = Some(target.host_request_id);
        tail.push(fact);
    }
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        ListeningResetMutation::Reset,
        NonEmptyActivityFacts::from_head_and_tail(head, tail),
        effects,
        Vec::new(),
    )
}
