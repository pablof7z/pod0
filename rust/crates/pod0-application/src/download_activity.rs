use pod0_domain::{
    ActivityCorrelationId, ActivityId, CommandId, EpisodeId, InternalCommandId, StateRevision,
};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    AuthorizedExternalEffect, DomainTransitionKind, DownloadIntentOrigin, DownloadTransition,
    DurableExternalEffectRequest, DurableInternalCommandRequest, ExternalEffectKind,
    NonEmptyActivityFacts, RequestDisposition, TransitionPlan, TransitionPlanError,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadEffectAuthorization {
    pub request: crate::DurableDownloadEffectRequest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadAdmissionActivityInput {
    pub command_id: CommandId,
    pub episode_id: EpisodeId,
    pub current_revision: StateRevision,
    pub legacy_replay: bool,
    pub state_changes: bool,
    pub admitted: bool,
    pub effect: Option<DownloadEffectAuthorization>,
    pub origin: DownloadIntentOrigin,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadInternalAdmissionActivityInput {
    pub internal_command_id: InternalCommandId,
    pub authorizing_activity_id: ActivityId,
    pub correlation_id: ActivityCorrelationId,
    pub episode_id: EpisodeId,
    pub current_revision: StateRevision,
    pub state_changes: bool,
    pub admitted: bool,
    pub effect: Option<DownloadEffectAuthorization>,
    pub disposition: RequestDisposition,
}

pub type DownloadAdmissionPlan =
    TransitionPlan<(), DurableExternalEffectRequest, DurableInternalCommandRequest>;

pub fn plan_download_admission(
    input: DownloadAdmissionActivityInput,
) -> Result<DownloadAdmissionPlan, TransitionPlanError> {
    let identity = crate::CommandActivityIdentity::new(input.command_id);
    let transaction_id = identity.transaction_id();
    let subject = ActivitySubject::Episode {
        episode_id: input.episode_id,
    };
    let (actor, origin) = activity_origin(input.origin);
    let disposition = if input.legacy_replay {
        RequestDisposition::Duplicate
    } else if !input.state_changes {
        RequestDisposition::NoSemanticChange
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
        actor,
        origin,
        subject,
        episode_id: Some(input.episode_id),
        fact,
    };
    if input.effect.is_some() && (!input.admitted || disposition != RequestDisposition::Accepted) {
        return Err(TransitionPlanError::InvalidEffectAuthorization);
    }
    let mut effects = Vec::new();
    let facts = if disposition == RequestDisposition::Accepted {
        let committed = StateRevision::new(
            input
                .current_revision
                .value
                .checked_add(1)
                .ok_or(TransitionPlanError::RevisionExhausted)?,
        );
        let mut tail = vec![base(
            1,
            ActivityFact::DomainTransition {
                kind: DomainTransitionKind::Download(if input.admitted {
                    DownloadTransition::AttemptStateChanged
                } else {
                    DownloadTransition::DesiredStateChanged
                }),
                previous_revision: input.current_revision,
                committed_revision: committed,
            },
        )];
        if let Some(effect) = input.effect {
            if effect.request.episode_id() != input.episode_id {
                return Err(TransitionPlanError::InvalidEffectAuthorization);
            }
            let intent_id = identity.effect_intent_id(0);
            tail.push(base(
                2,
                ActivityFact::EffectAuthorized {
                    intent_id,
                    kind: ExternalEffectKind::Download,
                },
            ));
            effects.push(AuthorizedExternalEffect {
                intent_id,
                authorizing_fact_index: 2,
                request: DurableExternalEffectRequest {
                    kind: ExternalEffectKind::Download,
                    subject,
                    episode_id: Some(input.episode_id),
                    not_before: effect.request.not_before,
                    deadline_at: effect.request.deadline_at,
                    execution: crate::DurableEffectExecution::Download {
                        request: effect.request,
                    },
                },
            });
        }
        NonEmptyActivityFacts::from_head_and_tail(
            base(0, ActivityFact::RequestDisposition { disposition }),
            tail,
        )
    } else {
        NonEmptyActivityFacts::new(base(0, ActivityFact::RequestDisposition { disposition }))
    };
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        (),
        facts,
        effects,
        Vec::new(),
    )
}

pub fn plan_download_internal_admission(
    input: DownloadInternalAdmissionActivityInput,
) -> Result<DownloadAdmissionPlan, TransitionPlanError> {
    let identity = crate::InternalCommandActivityIdentity::new(input.internal_command_id);
    let transaction_id = identity.transaction_id();
    let subject = ActivitySubject::Episode {
        episode_id: input.episode_id,
    };
    if input.state_changes != (input.disposition == RequestDisposition::Accepted) {
        return Err(TransitionPlanError::DispositionRequiresTransition);
    }
    let disposition = input.disposition;
    let base = |ordinal, fact| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: input.correlation_id,
        caused_by_activity_id: Some(input.authorizing_activity_id),
        command_id: None,
        host_request_id: None,
        actor: ActivityActor::System,
        origin: ActivityOrigin::InternalCommand,
        subject,
        episode_id: Some(input.episode_id),
        fact,
    };
    if input.effect.is_some() && (!input.admitted || !input.state_changes) {
        return Err(TransitionPlanError::InvalidEffectAuthorization);
    }
    let mut effects = Vec::new();
    let facts = if input.state_changes {
        let committed = StateRevision::new(
            input
                .current_revision
                .value
                .checked_add(1)
                .ok_or(TransitionPlanError::RevisionExhausted)?,
        );
        let mut tail = vec![base(
            1,
            ActivityFact::DomainTransition {
                kind: DomainTransitionKind::Download(if input.admitted {
                    DownloadTransition::AttemptStateChanged
                } else {
                    DownloadTransition::DesiredStateChanged
                }),
                previous_revision: input.current_revision,
                committed_revision: committed,
            },
        )];
        if let Some(effect) = input.effect {
            if effect.request.episode_id() != input.episode_id {
                return Err(TransitionPlanError::InvalidEffectAuthorization);
            }
            let intent_id = identity.effect_intent_id(0);
            tail.push(base(
                2,
                ActivityFact::EffectAuthorized {
                    intent_id,
                    kind: ExternalEffectKind::Download,
                },
            ));
            effects.push(AuthorizedExternalEffect {
                intent_id,
                authorizing_fact_index: 2,
                request: DurableExternalEffectRequest {
                    kind: ExternalEffectKind::Download,
                    subject,
                    episode_id: Some(input.episode_id),
                    not_before: effect.request.not_before,
                    deadline_at: effect.request.deadline_at,
                    execution: crate::DurableEffectExecution::Download {
                        request: effect.request,
                    },
                },
            });
        }
        NonEmptyActivityFacts::from_head_and_tail(
            base(0, ActivityFact::RequestDisposition { disposition }),
            tail,
        )
    } else {
        NonEmptyActivityFacts::new(base(0, ActivityFact::RequestDisposition { disposition }))
    };
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        (),
        facts,
        effects,
        Vec::new(),
    )
}

const fn activity_origin(origin: DownloadIntentOrigin) -> (ActivityActor, ActivityOrigin) {
    match origin {
        DownloadIntentOrigin::User => (ActivityActor::User, ActivityOrigin::UserInterface),
        DownloadIntentOrigin::Playback => (ActivityActor::System, ActivityOrigin::Playback),
        DownloadIntentOrigin::Automatic => (ActivityActor::System, ActivityOrigin::AutomaticPolicy),
        DownloadIntentOrigin::Unsupported { .. } => {
            (ActivityActor::System, ActivityOrigin::AutomaticPolicy)
        }
    }
}
