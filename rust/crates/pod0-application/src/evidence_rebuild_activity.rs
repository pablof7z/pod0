use pod0_domain::{CommandId, EpisodeId, StateRevision};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    AuthorizedExternalEffect, DomainTransitionKind, DurableEffectExecution,
    DurableEvidenceEmbeddingEffectRequest, DurableExternalEffectRequest,
    DurableInternalCommandRequest, ExternalEffectKind, NonEmptyActivityFacts,
    RecallKnowledgeTransition, RequestDisposition, TransitionPlan, TransitionPlanError,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvidenceRebuildActivityInput {
    pub command_id: CommandId,
    pub episode_id: EpisodeId,
    pub current_revision: StateRevision,
    pub semantic_change: bool,
    pub effect: Option<DurableEvidenceEmbeddingEffectRequest>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvidenceRebuildMutation {
    Apply,
    None,
}

pub type EvidenceRebuildPlan = TransitionPlan<
    EvidenceRebuildMutation,
    DurableExternalEffectRequest,
    DurableInternalCommandRequest,
>;

pub fn plan_evidence_rebuild(
    input: EvidenceRebuildActivityInput,
) -> Result<EvidenceRebuildPlan, TransitionPlanError> {
    let identity = crate::CommandActivityIdentity::new(input.command_id);
    let transaction_id = identity.transaction_id();
    let subject = ActivitySubject::Episode {
        episode_id: input.episode_id,
    };
    let fact = |ordinal, value| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: identity.correlation_id(),
        caused_by_activity_id: None,
        command_id: Some(input.command_id),
        host_request_id: None,
        actor: ActivityActor::User,
        origin: ActivityOrigin::UserInterface,
        subject,
        episode_id: Some(input.episode_id),
        fact: value,
    };
    let disposition = if input.semantic_change {
        RequestDisposition::Accepted
    } else {
        RequestDisposition::NoSemanticChange
    };
    let head = fact(0, ActivityFact::RequestDisposition { disposition });
    let (mutation, mut tail) = if input.semantic_change {
        let committed_revision = StateRevision::new(
            input
                .current_revision
                .value
                .checked_add(1)
                .ok_or(TransitionPlanError::RevisionExhausted)?,
        );
        (
            EvidenceRebuildMutation::Apply,
            vec![fact(
                1,
                ActivityFact::DomainTransition {
                    kind: DomainTransitionKind::RecallKnowledge(
                        RecallKnowledgeTransition::EvidenceGenerationChanged,
                    ),
                    previous_revision: input.current_revision,
                    committed_revision,
                },
            )],
        )
    } else {
        (EvidenceRebuildMutation::None, Vec::new())
    };
    let effects = input.effect.map_or_else(Vec::new, |request| {
        let intent_id = identity.effect_intent_id(0);
        let fact_index = 1 + tail.len();
        tail.push(fact(
            u8::try_from(fact_index).expect("bounded evidence facts"),
            ActivityFact::EffectAuthorized {
                intent_id,
                kind: ExternalEffectKind::RecallProvider,
            },
        ));
        vec![AuthorizedExternalEffect {
            intent_id,
            authorizing_fact_index: fact_index,
            request: DurableExternalEffectRequest {
                kind: ExternalEffectKind::RecallProvider,
                subject,
                episode_id: Some(input.episode_id),
                not_before: None,
                deadline_at: Some(request.deadline_at),
                execution: DurableEffectExecution::EvidenceEmbedding { request },
            },
        }]
    });
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        mutation,
        NonEmptyActivityFacts::from_head_and_tail(head, tail),
        effects,
        Vec::new(),
    )
}
