use pod0_domain::{
    ActivityCorrelationId, ActivityId, ActivityTransactionId, CommandId, EffectIntentId, EpisodeId,
    InternalCommandId, StateRevision,
};

use crate::{
    ActivityActor, ActivityDomain, ActivityFact, ActivityFactDraft, ActivityOrigin,
    ActivitySubject, AuthorizedExternalEffect, AuthorizedInternalCommand,
    DurableExternalEffectRequest, DurableInternalCommandRequest, ExternalEffectKind,
    InternalCommandKind, NonEmptyActivityFacts, RequestDisposition, TransitionDisposition,
    TransitionPlan, TransitionPlanError,
};

fn id(value: u64) -> ActivityId {
    ActivityId::from_parts(0, value)
}

fn transaction(value: u64) -> ActivityTransactionId {
    ActivityTransactionId::from_parts(0, value)
}

fn draft(
    value: u64,
    transaction_id: ActivityTransactionId,
    fact: ActivityFact,
) -> ActivityFactDraft {
    ActivityFactDraft {
        activity_id: id(value),
        transaction_id,
        correlation_id: ActivityCorrelationId::from_parts(0, 1),
        caused_by_activity_id: None,
        command_id: Some(CommandId::from_parts(0, 2)),
        host_request_id: None,
        actor: ActivityActor::User,
        origin: ActivityOrigin::UserInterface,
        subject: ActivitySubject::Episode {
            episode_id: EpisodeId::from_parts(0, 3),
        },
        episode_id: Some(EpisodeId::from_parts(0, 3)),
        fact,
    }
}

#[test]
fn non_empty_facts_cannot_represent_an_empty_transition() {
    let facts = NonEmptyActivityFacts::new(draft(
        1,
        transaction(1),
        ActivityFact::RequestDisposition {
            disposition: RequestDisposition::Accepted,
        },
    ));
    assert_eq!(facts.len(), 1);
}

#[test]
fn effect_requires_the_matching_authorization_fact() {
    let transaction_id = transaction(1);
    let effect_id = EffectIntentId::from_parts(0, 9);
    let facts = NonEmptyActivityFacts::new(draft(
        1,
        transaction_id,
        ActivityFact::RequestDisposition {
            disposition: RequestDisposition::Accepted,
        },
    ));
    let result = TransitionPlan::new(
        transaction_id,
        StateRevision::INITIAL,
        (),
        facts,
        vec![AuthorizedExternalEffect {
            intent_id: effect_id,
            authorizing_fact_index: 0,
            request: DurableExternalEffectRequest {
                kind: ExternalEffectKind::TranscriptProvider,
                subject: ActivitySubject::Episode {
                    episode_id: EpisodeId::from_parts(0, 3),
                },
                episode_id: Some(EpisodeId::from_parts(0, 3)),
                not_before: None,
                deadline_at: None,
                execution: exact_test_execution(),
            },
        }],
        Vec::<AuthorizedInternalCommand<DurableInternalCommandRequest>>::new(),
    );
    assert!(matches!(
        result,
        Err(TransitionPlanError::ExternalEffectAuthorizationMismatch { .. })
    ));
}

#[test]
fn matching_effect_and_internal_command_authorizations_are_valid() {
    let transaction_id = transaction(1);
    let effect_id = EffectIntentId::from_parts(0, 9);
    let command_id = InternalCommandId::from_parts(0, 10);
    let facts = NonEmptyActivityFacts::from_head_and_tail(
        draft(
            1,
            transaction_id,
            ActivityFact::RequestDisposition {
                disposition: RequestDisposition::Accepted,
            },
        ),
        vec![
            draft(
                2,
                transaction_id,
                ActivityFact::EffectAuthorized {
                    intent_id: effect_id,
                    kind: ExternalEffectKind::TranscriptProvider,
                },
            ),
            draft(
                3,
                transaction_id,
                ActivityFact::InternalCommandAuthorized {
                    internal_command_id: command_id,
                    target: ActivityDomain::RecallKnowledge,
                },
            ),
        ],
    );
    let result = TransitionPlan::new(
        transaction_id,
        StateRevision::INITIAL,
        "mutation",
        facts,
        vec![AuthorizedExternalEffect {
            intent_id: effect_id,
            authorizing_fact_index: 1,
            request: DurableExternalEffectRequest {
                kind: ExternalEffectKind::TranscriptProvider,
                subject: ActivitySubject::Episode {
                    episode_id: EpisodeId::from_parts(0, 3),
                },
                episode_id: Some(EpisodeId::from_parts(0, 3)),
                not_before: None,
                deadline_at: None,
                execution: exact_test_execution(),
            },
        }],
        vec![AuthorizedInternalCommand {
            internal_command_id: command_id,
            authorizing_fact_index: 2,
            command: DurableInternalCommandRequest {
                kind: InternalCommandKind::BuildTranscriptEvidence,
                target: ActivityDomain::RecallKnowledge,
                subject: ActivitySubject::Episode {
                    episode_id: EpisodeId::from_parts(0, 3),
                },
                episode_id: Some(EpisodeId::from_parts(0, 3)),
            },
        }],
    );
    assert!(result.is_ok());
}

fn exact_test_execution() -> crate::DurableEffectExecution {
    crate::DurableEffectExecution::Lifecycle {
        request: crate::DurableLifecycleEffectRequest {
            request_id: pod0_domain::HostRequestId::from_parts(90, 1),
            command_id: CommandId::from_parts(90, 2),
            cancellation_id: pod0_domain::CancellationId::from_parts(90, 3),
            issued_revision: StateRevision::INITIAL,
            wake_at: pod0_domain::UnixTimestampMilliseconds::new(1),
            reason: crate::CoreWakeReason::Unsupported { wire_code: 1 },
            attempt: 1,
        },
    }
}

#[test]
fn all_facts_must_share_the_commit_transaction_identity() {
    let facts = NonEmptyActivityFacts::new(draft(
        1,
        transaction(2),
        ActivityFact::RequestDisposition {
            disposition: RequestDisposition::Accepted,
        },
    ));
    let result = TransitionPlan::new(
        transaction(1),
        StateRevision::INITIAL,
        (),
        facts,
        Vec::<AuthorizedExternalEffect<DurableExternalEffectRequest>>::new(),
        Vec::<AuthorizedInternalCommand<DurableInternalCommandRequest>>::new(),
    );
    assert!(matches!(
        result,
        Err(TransitionPlanError::TransactionIdentityMismatch { .. })
    ));
}

#[test]
fn disposition_fact_cannot_disagree_with_its_typed_result() {
    let accepted = draft(
        1,
        transaction(1),
        ActivityFact::RequestDisposition {
            disposition: RequestDisposition::Accepted,
        },
    );
    assert!(TransitionDisposition::new(accepted, RequestDisposition::Accepted).is_some());
    assert!(TransitionDisposition::new(accepted, RequestDisposition::Duplicate).is_none());
}
