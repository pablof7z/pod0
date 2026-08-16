use pod0_domain::{
    ActivityCorrelationId, ActivityId, CommandId, EffectAttemptId, EffectIntentId, EpisodeId,
    HostRequestId, StateRevision,
};
use sha2::{Digest as _, Sha256};

use crate::{
    ActivityActor, ActivityDomain, ActivityFact, ActivityFactDraft, ActivityOrigin,
    ActivitySubject, AuthorizedExternalEffect, AuthorizedInternalCommand, DomainTransitionKind,
    DownloadEffectAuthorization, DownloadTransition, DurableExternalEffectRequest,
    DurableInternalCommandRequest, EffectObservationActivityIdentity, EffectOutcome,
    ExternalEffectKind, InternalCommandKind, NonEmptyActivityFacts, RequestDisposition,
    TransitionPlan, TransitionPlanError,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadFinalizationAuthorization {
    pub staged_file_path: String,
    pub claimed_byte_count: u64,
    pub sequence_number: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadObservationActivityInput {
    pub identity_attempt_id: EffectAttemptId,
    pub effect_attempt_id: EffectAttemptId,
    pub request_id: HostRequestId,
    pub command_id: CommandId,
    pub episode_id: EpisodeId,
    pub current_revision: StateRevision,
    pub intent_id: EffectIntentId,
    pub authorizing_activity_id: ActivityId,
    pub correlation_id: ActivityCorrelationId,
    pub outcome: EffectOutcome,
    pub state_changes: bool,
    pub next_effect: Option<DownloadEffectAuthorization>,
    pub finalization: Option<DownloadFinalizationAuthorization>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DownloadObservationMutation {
    Apply,
    RecordNoChange,
}

pub type DownloadObservationPlan = TransitionPlan<
    DownloadObservationMutation,
    DurableExternalEffectRequest,
    DurableInternalCommandRequest,
>;

pub fn plan_download_observation(
    input: DownloadObservationActivityInput,
) -> Result<DownloadObservationPlan, TransitionPlanError> {
    let identity = EffectObservationActivityIdentity::new(input.identity_attempt_id);
    let transaction_id = identity.transaction_id();
    let subject = ActivitySubject::Episode {
        episode_id: input.episode_id,
    };
    if input.next_effect.is_some() && !input.state_changes {
        return Err(TransitionPlanError::InvalidEffectAuthorization);
    }
    if input.finalization.is_some() && !input.state_changes {
        return Err(TransitionPlanError::DispositionRequiresTransition);
    }
    if input
        .next_effect
        .as_ref()
        .is_some_and(|effect| effect.request.episode_id() != input.episode_id)
    {
        return Err(TransitionPlanError::InvalidEffectAuthorization);
    }
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
    let mut tail = vec![base(
        1,
        ActivityFact::EffectObserved {
            intent_id: input.intent_id,
            attempt_id: input.effect_attempt_id,
            outcome: input.outcome,
        },
    )];
    if input.state_changes {
        let committed_revision = StateRevision::new(
            input
                .current_revision
                .value
                .checked_add(1)
                .ok_or(TransitionPlanError::RevisionExhausted)?,
        );
        tail.push(base(
            2,
            ActivityFact::DomainTransition {
                kind: DomainTransitionKind::Download(DownloadTransition::AttemptStateChanged),
                previous_revision: input.current_revision,
                committed_revision,
            },
        ));
    }
    let effects = input.next_effect.map_or_else(Vec::new, |effect| {
        let intent_id = identity.effect_intent_id(0);
        let fact_index = tail.len() + 1;
        tail.push(base(
            u8::try_from(fact_index).expect("bounded download facts"),
            ActivityFact::EffectAuthorized {
                intent_id,
                kind: ExternalEffectKind::Download,
            },
        ));
        vec![AuthorizedExternalEffect {
            intent_id,
            authorizing_fact_index: fact_index,
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
        }]
    });
    let commands = input.finalization.map_or_else(Vec::new, |finalization| {
        let internal_command_id = identity.internal_command_id(0);
        let fact_index = tail.len() + 1;
        tail.push(base(
            u8::try_from(fact_index).expect("bounded download facts"),
            ActivityFact::InternalCommandAuthorized {
                internal_command_id,
                target: ActivityDomain::Download,
            },
        ));
        vec![AuthorizedInternalCommand {
            internal_command_id,
            authorizing_fact_index: fact_index,
            command: DurableInternalCommandRequest {
                kind: InternalCommandKind::FinalizeDownloadArtifact {
                    request_id: input.request_id,
                    sequence_number: finalization.sequence_number,
                    staged_file_path: finalization.staged_file_path,
                    claimed_byte_count: finalization.claimed_byte_count,
                },
                target: ActivityDomain::Download,
                subject,
                episode_id: Some(input.episode_id),
            },
        }]
    });
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        if input.state_changes {
            DownloadObservationMutation::Apply
        } else {
            DownloadObservationMutation::RecordNoChange
        },
        NonEmptyActivityFacts::from_head_and_tail(
            base(
                0,
                ActivityFact::RequestDisposition {
                    disposition: RequestDisposition::Accepted,
                },
            ),
            tail,
        ),
        effects,
        commands,
    )
}

pub fn download_observation_identity(
    attempt_id: EffectAttemptId,
    sequence_number: u64,
) -> EffectAttemptId {
    let mut hash = Sha256::new();
    hash.update(b"pod0/download-observation/v1");
    hash.update(attempt_id.into_bytes());
    hash.update(sequence_number.to_be_bytes());
    let digest: [u8; 32] = hash.finalize().into();
    EffectAttemptId::from_bytes(digest[..16].try_into().expect("fixed digest prefix"))
}
