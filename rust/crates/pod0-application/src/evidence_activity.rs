use pod0_domain::{
    ActivityCorrelationId, ActivityId, EpisodeId, EvidenceGenerationId, InternalCommandId,
    StateRevision, TranscriptEvidenceArtifact, UnixTimestampMilliseconds,
};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    AuthorizedExternalEffect, DomainTransitionKind, DurableExternalEffectRequest,
    DurableInternalCommandRequest, ExternalEffectKind, InternalCommandActivityIdentity,
    NonEmptyActivityFacts, RecallKnowledgeTransition, RequestDisposition, TransitionPlan,
    TransitionPlanError,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvidenceAdmissionActivityInput {
    pub internal_command_id: InternalCommandId,
    pub authorizing_activity_id: ActivityId,
    pub correlation_id: ActivityCorrelationId,
    pub episode_id: EpisodeId,
    pub artifact: TranscriptEvidenceArtifact,
    pub deadline_at: UnixTimestampMilliseconds,
}

pub type EvidenceAdmissionPlan = TransitionPlan<
    TranscriptEvidenceArtifact,
    DurableExternalEffectRequest,
    DurableInternalCommandRequest,
>;

pub fn plan_evidence_admission(
    input: EvidenceAdmissionActivityInput,
) -> Result<EvidenceAdmissionPlan, TransitionPlanError> {
    let identity = InternalCommandActivityIdentity::new(input.internal_command_id);
    let transaction_id = identity.transaction_id();
    let intent_id = identity.effect_intent_id(0);
    let subject = ActivitySubject::Episode {
        episode_id: input.episode_id,
    };
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
    TransitionPlan::new(
        transaction_id,
        StateRevision::INITIAL,
        input.artifact,
        NonEmptyActivityFacts::from_head_and_tail(
            base(
                0,
                ActivityFact::RequestDisposition {
                    disposition: RequestDisposition::Accepted,
                },
            ),
            vec![
                base(
                    1,
                    ActivityFact::DomainTransition {
                        kind: DomainTransitionKind::RecallKnowledge(
                            RecallKnowledgeTransition::EvidenceGenerationChanged,
                        ),
                        previous_revision: StateRevision::INITIAL,
                        committed_revision: StateRevision::new(1),
                    },
                ),
                base(
                    2,
                    ActivityFact::EffectAuthorized {
                        intent_id,
                        kind: ExternalEffectKind::RecallProvider,
                    },
                ),
            ],
        ),
        vec![AuthorizedExternalEffect {
            intent_id,
            authorizing_fact_index: 2,
            request: DurableExternalEffectRequest {
                kind: ExternalEffectKind::RecallProvider,
                subject,
                episode_id: Some(input.episode_id),
                not_before: None,
                deadline_at: Some(input.deadline_at),
            },
        }],
        Vec::new(),
    )
}

#[must_use]
pub fn evidence_phase_command_id(
    generation_id: EvidenceGenerationId,
    phase: &[u8],
) -> pod0_domain::CommandId {
    use sha2::{Digest as _, Sha256};

    let mut hash = Sha256::new();
    hash.update(b"pod0-evidence-rebuild-phase-v1\0");
    hash.update(generation_id.into_bytes());
    hash.update(phase);
    let digest: [u8; 32] = hash.finalize().into();
    pod0_domain::CommandId::from_bytes(digest[..16].try_into().expect("fixed digest prefix"))
}
