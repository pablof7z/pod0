use pod0_domain::{
    ActivityCorrelationId, ActivityId, CommandId, EffectAttemptId, EffectIntentId, EpisodeId,
    HostRequestId, StateRevision,
};

use crate::{
    ActivityActor, ActivityDomain, ActivityFact, ActivityFactDraft, ActivityOrigin,
    ActivitySubject, AuthorizedExternalEffect, AuthorizedInternalCommand, ChapterTransition,
    ChapterWorkflowEffectAuthorization, DomainTransitionKind, DurableExternalEffectRequest,
    DurableInternalCommandRequest, EffectObservationActivityIdentity, EffectOutcome,
    ExternalEffectKind, NonEmptyActivityFacts, RequestDisposition, TransitionPlan,
    TransitionPlanError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChapterRecordedTransition {
    pub kind: ChapterTransition,
    pub previous_revision: StateRevision,
    pub committed_revision: StateRevision,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChapterEffectObservationActivityInput {
    pub identity_attempt_id: EffectAttemptId,
    pub request_id: HostRequestId,
    pub command_id: CommandId,
    pub episode_id: EpisodeId,
    pub current_revision: StateRevision,
    pub intent_id: EffectIntentId,
    pub attempt_id: EffectAttemptId,
    pub authorizing_activity_id: ActivityId,
    pub correlation_id: ActivityCorrelationId,
    pub outcome: EffectOutcome,
    pub transitions: Vec<ChapterRecordedTransition>,
    pub next_effect: Option<ChapterWorkflowEffectAuthorization>,
    pub authorize_finalization: bool,
    pub effect_kind: ExternalEffectKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChapterObservationMutation {
    Apply,
    RecordNoChange,
}

pub type ChapterEffectObservationPlan = TransitionPlan<
    ChapterObservationMutation,
    DurableExternalEffectRequest,
    DurableInternalCommandRequest,
>;

pub fn plan_chapter_effect_observation(
    input: ChapterEffectObservationActivityInput,
) -> Result<ChapterEffectObservationPlan, TransitionPlanError> {
    let disposition = if input.transitions.is_empty() {
        RequestDisposition::NoSemanticChange
    } else {
        RequestDisposition::Accepted
    };
    if input.transitions.is_empty() && input.next_effect.is_some() {
        return Err(TransitionPlanError::DispositionRequiresTransition);
    }
    if !matches!(
        input.effect_kind,
        ExternalEffectKind::PublisherChapterProvider | ExternalEffectKind::ModelChapterProvider
    ) {
        return Err(TransitionPlanError::InvalidEffectAuthorization);
    }
    let identity = EffectObservationActivityIdentity::new(input.identity_attempt_id);
    let transaction_id = identity.transaction_id();
    let committed_revision = StateRevision::new(
        input
            .current_revision
            .value
            .checked_add(1)
            .ok_or(TransitionPlanError::RevisionExhausted)?,
    );
    let subject = ActivitySubject::Episode {
        episode_id: input.episode_id,
    };
    let base = |ordinal, fact| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: input.correlation_id,
        caused_by_activity_id: Some(input.authorizing_activity_id),
        command_id: Some(input.command_id),
        host_request_id: Some(input.request_id),
        actor: ActivityActor::System,
        origin: ActivityOrigin::HostObservation,
        subject,
        episode_id: Some(input.episode_id),
        fact,
    };
    if let Some(first) = input.transitions.first()
        && *first
            != (ChapterRecordedTransition {
                kind: first.kind,
                previous_revision: input.current_revision,
                committed_revision,
            })
    {
        return Err(TransitionPlanError::DispositionRequiresTransition);
    }
    let mut tail = vec![base(
        1,
        ActivityFact::EffectObserved {
            intent_id: input.intent_id,
            attempt_id: input.attempt_id,
            outcome: input.outcome,
        },
    )];
    for transition in &input.transitions {
        let ordinal = u8::try_from(tail.len() + 1)
            .map_err(|_| TransitionPlanError::TooManyInternalCommands)?;
        tail.push(base(
            ordinal,
            ActivityFact::DomainTransition {
                kind: DomainTransitionKind::Chapter(transition.kind),
                previous_revision: transition.previous_revision,
                committed_revision: transition.committed_revision,
            },
        ));
    }
    let effects = input.next_effect.map_or_else(Vec::new, |effect| {
        let intent_id = identity.effect_intent_id(0);
        let fact_index = tail.len() + 1;
        tail.push(base(
            u8::try_from(fact_index).expect("bounded chapter observation facts"),
            ActivityFact::EffectAuthorized {
                intent_id,
                kind: input.effect_kind,
            },
        ));
        vec![AuthorizedExternalEffect {
            intent_id,
            authorizing_fact_index: fact_index,
            request: DurableExternalEffectRequest {
                kind: input.effect_kind,
                subject,
                episode_id: Some(input.episode_id),
                not_before: effect.not_before,
                deadline_at: effect.deadline_at,
                execution: match effect.execution {
                    crate::ChapterWorkflowExecution::Publisher(request) => {
                        crate::DurableEffectExecution::PublisherChapter { request }
                    }
                    crate::ChapterWorkflowExecution::Model(request) => {
                        crate::DurableEffectExecution::ModelChapter { request }
                    }
                },
            },
        }]
    });
    let commands = if input.authorize_finalization {
        let internal_command_id = identity.internal_command_id(0);
        let fact_index = tail.len() + 1;
        tail.push(base(
            u8::try_from(fact_index).expect("bounded chapter observation facts"),
            ActivityFact::InternalCommandAuthorized {
                internal_command_id,
                target: ActivityDomain::Chapter,
            },
        ));
        vec![AuthorizedInternalCommand {
            internal_command_id,
            authorizing_fact_index: fact_index,
            command: DurableInternalCommandRequest {
                kind: crate::InternalCommandKind::FinalizeModelChapters {
                    request_id: input.request_id,
                },
                target: ActivityDomain::Chapter,
                subject,
                episode_id: Some(input.episode_id),
            },
        }]
    } else {
        Vec::new()
    };
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        if input.transitions.is_empty() {
            ChapterObservationMutation::RecordNoChange
        } else {
            ChapterObservationMutation::Apply
        },
        NonEmptyActivityFacts::from_head_and_tail(
            base(0, ActivityFact::RequestDisposition { disposition }),
            tail,
        ),
        effects,
        commands,
    )
}
