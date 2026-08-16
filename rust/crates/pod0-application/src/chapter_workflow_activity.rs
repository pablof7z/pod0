use pod0_domain::{CommandId, EpisodeId, StateRevision, UnixTimestampMilliseconds};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    AuthorizedExternalEffect, ChapterTransition, CommandActivityIdentity, DomainTransitionKind,
    DurableExternalEffectRequest, DurableInternalCommandRequest, ExternalEffectKind,
    NonEmptyActivityFacts, RequestDisposition, TransitionPlan, TransitionPlanError,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChapterWorkflowEffectAuthorization {
    pub not_before: Option<UnixTimestampMilliseconds>,
    pub deadline_at: Option<UnixTimestampMilliseconds>,
    pub execution: ChapterWorkflowExecution,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChapterWorkflowExecution {
    Publisher(crate::DurablePublisherChapterEffectRequest),
    Model(crate::DurableModelChapterEffectRequest),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChapterWorkflowActivityInput {
    pub identity_command_id: CommandId,
    pub command_id: CommandId,
    pub episode_id: EpisodeId,
    pub current_revision: StateRevision,
    pub disposition: RequestDisposition,
    pub transition: Option<ChapterTransition>,
    pub effect: Option<ChapterWorkflowEffectAuthorization>,
    pub effect_kind: ExternalEffectKind,
    pub actor: ActivityActor,
    pub origin: ActivityOrigin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChapterWorkflowMutation {
    Apply,
    RecordNoChange,
}

pub type ChapterWorkflowActivityPlan = TransitionPlan<
    ChapterWorkflowMutation,
    DurableExternalEffectRequest,
    DurableInternalCommandRequest,
>;

pub fn plan_chapter_workflow_activity(
    input: ChapterWorkflowActivityInput,
) -> Result<ChapterWorkflowActivityPlan, TransitionPlanError> {
    if !matches!(
        input.effect_kind,
        ExternalEffectKind::PublisherChapterProvider | ExternalEffectKind::ModelChapterProvider
    ) {
        return Err(TransitionPlanError::InvalidEffectAuthorization);
    }
    if input.effect.is_some()
        && (input.transition.is_none() || input.disposition != RequestDisposition::Accepted)
    {
        return Err(TransitionPlanError::DispositionRequiresTransition);
    }
    let identity = CommandActivityIdentity::new(input.identity_command_id);
    let transaction_id = identity.transaction_id();
    let subject = ActivitySubject::Episode {
        episode_id: input.episode_id,
    };
    let base = |ordinal, fact| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: identity.correlation_id(),
        caused_by_activity_id: None,
        command_id: Some(input.command_id),
        host_request_id: None,
        actor: input.actor,
        origin: input.origin,
        subject,
        episode_id: Some(input.episode_id),
        fact,
    };
    let mut tail = Vec::new();
    if let Some(transition) = input.transition {
        let committed_revision = StateRevision::new(
            input
                .current_revision
                .value
                .checked_add(1)
                .ok_or(TransitionPlanError::RevisionExhausted)?,
        );
        tail.push(base(
            1,
            ActivityFact::DomainTransition {
                kind: DomainTransitionKind::Chapter(transition),
                previous_revision: input.current_revision,
                committed_revision,
            },
        ));
    }
    let effects = input.effect.map_or_else(Vec::new, |effect| {
        let intent_id = identity.effect_intent_id(0);
        let fact_index = 1 + tail.len();
        tail.push(base(
            u8::try_from(fact_index).expect("bounded chapter activity facts"),
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
                    ChapterWorkflowExecution::Publisher(request) => {
                        crate::DurableEffectExecution::PublisherChapter { request }
                    }
                    ChapterWorkflowExecution::Model(request) => {
                        crate::DurableEffectExecution::ModelChapter { request }
                    }
                },
            },
        }]
    });
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        if input.transition.is_some() {
            ChapterWorkflowMutation::Apply
        } else {
            ChapterWorkflowMutation::RecordNoChange
        },
        NonEmptyActivityFacts::from_head_and_tail(
            base(
                0,
                ActivityFact::RequestDisposition {
                    disposition: input.disposition,
                },
            ),
            tail,
        ),
        effects,
        Vec::new(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use pod0_domain::{CommandId, EpisodeId, StateRevision, UnixTimestampMilliseconds};

    #[test]
    fn accepted_transition_authorizes_one_chapter_effect() {
        let deadline = UnixTimestampMilliseconds::new(500);
        let plan = plan_chapter_workflow_activity(ChapterWorkflowActivityInput {
            identity_command_id: CommandId::from_parts(1, 1),
            command_id: CommandId::from_parts(1, 1),
            episode_id: EpisodeId::from_parts(2, 1),
            current_revision: StateRevision::new(7),
            disposition: RequestDisposition::Accepted,
            transition: Some(ChapterTransition::PublisherWorkflowStateChanged),
            effect: Some(ChapterWorkflowEffectAuthorization {
                not_before: None,
                deadline_at: Some(deadline),
                execution: test_execution(deadline),
            }),
            effect_kind: ExternalEffectKind::PublisherChapterProvider,
            actor: ActivityActor::User,
            origin: ActivityOrigin::UserInterface,
        })
        .unwrap();
        let (_, expected, mutation, facts, effects, commands, disposition) = plan.into_parts();
        assert_eq!(expected, StateRevision::new(7));
        assert_eq!(mutation, ChapterWorkflowMutation::Apply);
        assert_eq!(disposition, RequestDisposition::Accepted);
        assert!(commands.is_empty());
        assert_eq!(effects.len(), 1);
        assert_eq!(
            effects[0].request.kind,
            ExternalEffectKind::PublisherChapterProvider
        );
        assert_eq!(effects[0].request.deadline_at, Some(deadline));
        assert!(facts.iter().any(|fact| matches!(
            fact.fact,
            ActivityFact::DomainTransition {
                kind: DomainTransitionKind::Chapter(
                    ChapterTransition::PublisherWorkflowStateChanged
                ),
                previous_revision,
                committed_revision,
            } if previous_revision == StateRevision::new(7)
                && committed_revision == StateRevision::new(8)
        )));
    }

    #[test]
    fn no_change_cannot_authorize_an_effect() {
        let input = ChapterWorkflowActivityInput {
            identity_command_id: CommandId::from_parts(1, 2),
            command_id: CommandId::from_parts(1, 2),
            episode_id: EpisodeId::from_parts(2, 2),
            current_revision: StateRevision::new(4),
            disposition: RequestDisposition::NoSemanticChange,
            transition: None,
            effect: None,
            effect_kind: ExternalEffectKind::PublisherChapterProvider,
            actor: ActivityActor::User,
            origin: ActivityOrigin::UserInterface,
        };
        let plan = plan_chapter_workflow_activity(input.clone()).unwrap();
        let (_, expected, mutation, facts, effects, _, disposition) = plan.into_parts();
        assert_eq!(expected, StateRevision::new(4));
        assert_eq!(mutation, ChapterWorkflowMutation::RecordNoChange);
        assert_eq!(disposition, RequestDisposition::NoSemanticChange);
        assert_eq!(facts.len(), 1);
        assert!(effects.is_empty());
        assert_eq!(
            plan_chapter_workflow_activity(ChapterWorkflowActivityInput {
                effect: Some(ChapterWorkflowEffectAuthorization {
                    not_before: None,
                    deadline_at: None,
                    execution: test_execution(UnixTimestampMilliseconds::new(500)),
                }),
                ..input
            }),
            Err(TransitionPlanError::DispositionRequiresTransition)
        );
    }

    fn test_execution(deadline: UnixTimestampMilliseconds) -> ChapterWorkflowExecution {
        ChapterWorkflowExecution::Publisher(crate::DurablePublisherChapterEffectRequest {
            request_id: pod0_domain::HostRequestId::from_parts(3, 1),
            command_id: CommandId::from_parts(1, 1),
            cancellation_id: pod0_domain::CancellationId::from_parts(4, 1),
            issued_revision: StateRevision::new(1),
            deadline_at: Some(deadline),
            episode_id: EpisodeId::from_parts(2, 1),
            source_url: "https://example.com/chapters.json".to_owned(),
            not_before: None,
            maximum_response_bytes: 1024,
        })
    }
}
