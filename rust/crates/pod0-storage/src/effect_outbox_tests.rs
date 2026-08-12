use pod0_application::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    AuthorizedExternalEffect, DurableExternalEffectRequest, DurableInternalCommandRequest,
    EffectOutcome, ExternalEffectKind, NonEmptyActivityFacts, RequestDisposition, TransitionPlan,
};
use pod0_domain::{
    ActivityCorrelationId, ActivityId, ActivityTransactionId, CommandId, ContentDigest,
    EffectAttemptId, EffectIntentId, EffectLeaseId, EpisodeId, StateRevision,
    UnixTimestampMilliseconds,
};
use rusqlite::{Connection, params};

use crate::recovery_test_support::Fixture;
use crate::{
    EffectOutbox, EffectOutboxError, TransitionCommit, TransitionIngress, TransitionIngressKind,
};

fn commit_effect(path: &std::path::Path) -> EffectIntentId {
    let episode_id = EpisodeId::from_parts(10, 1);
    let transaction_id = ActivityTransactionId::from_parts(11, 1);
    let correlation_id = ActivityCorrelationId::from_parts(12, 1);
    let intent_id = EffectIntentId::from_parts(13, 1);
    let base = |index, fact| ActivityFactDraft {
        activity_id: ActivityId::from_parts(14, index),
        transaction_id,
        correlation_id,
        caused_by_activity_id: None,
        command_id: Some(CommandId::from_parts(15, 1)),
        host_request_id: None,
        actor: ActivityActor::System,
        origin: ActivityOrigin::AutomaticPolicy,
        subject: ActivitySubject::Episode { episode_id },
        episode_id: Some(episode_id),
        fact,
    };
    let request = DurableExternalEffectRequest {
        kind: ExternalEffectKind::TranscriptProvider,
        subject: ActivitySubject::Episode { episode_id },
        episode_id: Some(episode_id),
        not_before: Some(UnixTimestampMilliseconds::new(1_000)),
        deadline_at: Some(UnixTimestampMilliseconds::new(10_000)),
    };
    let plan = TransitionPlan::new(
        transaction_id,
        StateRevision::INITIAL,
        (),
        NonEmptyActivityFacts::from_head_and_tail(
            base(
                1,
                ActivityFact::RequestDisposition {
                    disposition: RequestDisposition::Accepted,
                },
            ),
            vec![base(
                2,
                ActivityFact::EffectAuthorized {
                    intent_id,
                    kind: ExternalEffectKind::TranscriptProvider,
                },
            )],
        ),
        vec![AuthorizedExternalEffect {
            intent_id,
            authorizing_fact_index: 1,
            request,
        }],
        Vec::<pod0_application::AuthorizedInternalCommand<DurableInternalCommandRequest>>::new(),
    )
    .unwrap();
    TransitionCommit::open(path)
        .unwrap()
        .commit_no_state_change(
            TransitionIngress {
                kind: TransitionIngressKind::ScheduledWake,
                id: CommandId::from_parts(15, 1).into_bytes(),
                fingerprint: ContentDigest::from_bytes([16; 32]),
            },
            plan,
            UnixTimestampMilliseconds::new(900),
        )
        .unwrap();
    intent_id
}

#[test]
fn lease_excludes_concurrent_execution_and_expiry_reclaims_with_a_new_fence() {
    let fixture = Fixture::new();
    fixture.migrate_to_current(50).unwrap();
    let intent_id = commit_effect(&fixture.store);
    let outbox = EffectOutbox::open(&fixture.store).unwrap();
    let first = outbox
        .claim_next(
            EffectAttemptId::from_parts(20, 1),
            EffectLeaseId::from_parts(21, 1),
            UnixTimestampMilliseconds::new(1_000),
            1_000,
        )
        .unwrap()
        .unwrap();
    assert_eq!(first.intent_id, intent_id);
    assert_eq!(first.fence, 1);
    assert!(
        outbox
            .claim_next(
                EffectAttemptId::from_parts(20, 2),
                EffectLeaseId::from_parts(21, 2),
                UnixTimestampMilliseconds::new(1_500),
                1_000,
            )
            .unwrap()
            .is_none()
    );

    let reopened = EffectOutbox::open(&fixture.store).unwrap();
    let second = reopened
        .claim_next(
            EffectAttemptId::from_parts(20, 3),
            EffectLeaseId::from_parts(21, 3),
            UnixTimestampMilliseconds::new(2_001),
            1_000,
        )
        .unwrap()
        .unwrap();
    assert_eq!(second.intent_id, intent_id);
    assert_eq!(second.fence, 2);
    assert!(matches!(
        reopened.stage_observation(
            first.lease_id,
            first.fence,
            EffectOutcome::Succeeded,
            UnixTimestampMilliseconds::new(1_900),
        ),
        Err(EffectOutboxError::StaleLease)
    ));
    reopened
        .stage_observation(
            second.lease_id,
            second.fence,
            EffectOutcome::Succeeded,
            UnixTimestampMilliseconds::new(2_500),
        )
        .unwrap();
    assert!(
        reopened
            .claim_next(
                EffectAttemptId::from_parts(20, 4),
                EffectLeaseId::from_parts(21, 4),
                UnixTimestampMilliseconds::new(4_000),
                1_000,
            )
            .unwrap()
            .is_none()
    );
}

#[test]
fn database_rejects_an_effect_without_its_exact_authorization_fact() {
    let fixture = Fixture::new();
    fixture.migrate_to_current(51).unwrap();
    let connection = Connection::open(&fixture.store).unwrap();
    connection
        .pragma_update(None, "foreign_keys", true)
        .unwrap();
    let result = connection.execute(
        "INSERT INTO pod0_effect_intents(intent_id,authorizing_activity_id,correlation_id,\
         effect_kind_code,subject_code,request_json,available_at_ms,committed_at_ms) \
         VALUES(?1,?2,?3,7,0,'{}',1000,900)",
        params![
            EffectIntentId::from_parts(1, 1).into_bytes().as_slice(),
            ActivityId::from_parts(2, 2).into_bytes().as_slice(),
            ActivityCorrelationId::from_parts(3, 3)
                .into_bytes()
                .as_slice(),
        ],
    );
    assert!(result.is_err());
}

#[test]
fn lease_duration_is_bounded_before_any_write() {
    let fixture = Fixture::new();
    fixture.migrate_to_current(52).unwrap();
    commit_effect(&fixture.store);
    let outbox = EffectOutbox::open(&fixture.store).unwrap();
    assert_eq!(
        outbox.claim_next(
            EffectAttemptId::from_parts(1, 1),
            EffectLeaseId::from_parts(1, 2),
            UnixTimestampMilliseconds::new(1_000),
            999,
        ),
        Err(EffectOutboxError::InvalidLeaseDuration)
    );
}
