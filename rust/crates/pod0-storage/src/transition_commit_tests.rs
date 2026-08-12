use pod0_application::{
    ActivityActor, ActivityDomain, ActivityFact, ActivityFactDraft, ActivityOrigin,
    ActivitySubject, AuthorizedExternalEffect, AuthorizedInternalCommand,
    DurableExternalEffectRequest, DurableInternalCommandRequest, ExternalEffectKind,
    NonEmptyActivityFacts, RequestDisposition, RequestRejectionReason, TransitionPlan,
};
use pod0_domain::{
    ActivityCorrelationId, ActivityId, ActivityTransactionId, CommandId, ContentDigest,
    EffectIntentId, EpisodeId, InternalCommandId, StateRevision, UnixTimestampMilliseconds,
};
use rusqlite::Connection;

use super::{CommitFaultPoint, TransitionCommit};
use crate::recovery_test_support::Fixture;
use crate::{StorageError, TransitionIngress, TransitionIngressKind};

fn draft(index: u64, fact: ActivityFact) -> ActivityFactDraft {
    let episode_id = EpisodeId::from_parts(5, 6);
    ActivityFactDraft {
        activity_id: ActivityId::from_parts(1, index),
        transaction_id: ActivityTransactionId::from_parts(2, 1),
        correlation_id: ActivityCorrelationId::from_parts(3, 1),
        caused_by_activity_id: None,
        command_id: Some(CommandId::from_parts(4, 1)),
        host_request_id: None,
        actor: ActivityActor::User,
        origin: ActivityOrigin::UserInterface,
        subject: ActivitySubject::Episode { episode_id },
        episode_id: Some(episode_id),
        fact,
    }
}

fn plan()
-> TransitionPlan<&'static str, DurableExternalEffectRequest, DurableInternalCommandRequest> {
    let effect_id = EffectIntentId::from_parts(7, 1);
    let command_id = InternalCommandId::from_parts(8, 1);
    let episode_id = EpisodeId::from_parts(5, 6);
    TransitionPlan::new(
        ActivityTransactionId::from_parts(2, 1),
        StateRevision::new(9),
        "state",
        NonEmptyActivityFacts::from_head_and_tail(
            draft(
                1,
                ActivityFact::RequestDisposition {
                    disposition: RequestDisposition::Accepted,
                },
            ),
            vec![
                draft(
                    2,
                    ActivityFact::EffectAuthorized {
                        intent_id: effect_id,
                        kind: ExternalEffectKind::TranscriptProvider,
                    },
                ),
                draft(
                    3,
                    ActivityFact::InternalCommandAuthorized {
                        internal_command_id: command_id,
                        target: ActivityDomain::RecallKnowledge,
                    },
                ),
            ],
        ),
        vec![AuthorizedExternalEffect {
            intent_id: effect_id,
            authorizing_fact_index: 1,
            request: DurableExternalEffectRequest {
                kind: ExternalEffectKind::TranscriptProvider,
                subject: ActivitySubject::Episode { episode_id },
                episode_id: Some(episode_id),
                not_before: None,
                deadline_at: None,
            },
        }],
        vec![AuthorizedInternalCommand {
            internal_command_id: command_id,
            authorizing_fact_index: 2,
            command: DurableInternalCommandRequest {
                kind: pod0_application::InternalCommandKind::BuildTranscriptEvidence,
                target: ActivityDomain::RecallKnowledge,
                subject: ActivitySubject::Episode { episode_id },
                episode_id: Some(episode_id),
            },
        }],
    )
    .unwrap()
}

fn ingress(fingerprint: u8) -> TransitionIngress {
    TransitionIngress {
        kind: TransitionIngressKind::ApplicationCommand,
        id: CommandId::from_parts(4, 1).into_bytes(),
        fingerprint: ContentDigest::from_bytes([fingerprint; 32]),
    }
}

fn count(connection: &Connection, table: &str) -> i64 {
    connection
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
}

#[test]
fn state_facts_outboxes_and_receipt_commit_atomically_and_replay() {
    let fixture = Fixture::new();
    fixture.migrate_to_current(20).unwrap();
    let connection = Connection::open(&fixture.store).unwrap();
    connection
        .execute("CREATE TABLE test_state(value TEXT)", [])
        .unwrap();
    drop(connection);
    let committer = TransitionCommit::open(&fixture.store).unwrap();
    let receipt = committer
        .commit_with(
            ingress(1),
            plan(),
            UnixTimestampMilliseconds::new(100),
            |transaction, expected, value| {
                assert_eq!(expected, StateRevision::new(9));
                transaction
                    .execute("INSERT INTO test_state VALUES(?1)", [value])
                    .unwrap();
                Ok(StateRevision::new(10))
            },
        )
        .unwrap();
    assert!(!receipt.replayed);
    assert_eq!(receipt.first_sequence, 1);
    assert_eq!(receipt.last_sequence, 3);
    assert_eq!(receipt.committed_revision, StateRevision::new(10));

    let replay = committer
        .commit_with(
            ingress(1),
            plan(),
            UnixTimestampMilliseconds::new(101),
            |_, _, _| panic!("replay must not mutate"),
        )
        .unwrap();
    assert!(replay.replayed);
    let connection = Connection::open(&fixture.store).unwrap();
    assert_eq!(count(&connection, "test_state"), 1);
    assert_eq!(count(&connection, "pod0_activity_facts"), 3);
    assert_eq!(count(&connection, "pod0_effect_intents"), 1);
    assert_eq!(count(&connection, "pod0_internal_command_intents"), 1);
    assert_eq!(count(&connection, "pod0_transition_receipts"), 1);
    assert!(matches!(
        committer.commit_with(
            ingress(2),
            plan(),
            UnixTimestampMilliseconds::new(102),
            |_, _, _| unreachable!()
        ),
        Err(StorageError::ActivityCommandConflict)
    ));
}

#[test]
fn state_dependent_planning_runs_under_the_immediate_write_lock() {
    let fixture = Fixture::new();
    fixture.migrate_to_current(21).unwrap();
    let connection = Connection::open(&fixture.store).unwrap();
    connection
        .execute("CREATE TABLE test_state(value TEXT)", [])
        .unwrap();
    drop(connection);

    let competing_path = fixture.store.clone();
    let receipt = TransitionCommit::open(&fixture.store)
        .unwrap()
        .commit_planned_with(
            ingress(1),
            UnixTimestampMilliseconds::new(100),
            move |_| {
                let competing = Connection::open(competing_path).unwrap();
                competing.busy_timeout(std::time::Duration::ZERO).unwrap();
                assert!(
                    competing
                        .execute("INSERT INTO test_state VALUES('bypass')", [])
                        .is_err(),
                    "planning must already hold the immediate writer lock"
                );
                Ok(plan())
            },
            |transaction, _, value| {
                transaction
                    .execute("INSERT INTO test_state VALUES(?1)", [value])
                    .unwrap();
                Ok(StateRevision::new(10))
            },
        )
        .unwrap();

    assert_eq!(receipt.committed_revision, StateRevision::new(10));
    let connection = Connection::open(&fixture.store).unwrap();
    assert_eq!(count(&connection, "test_state"), 1);
    assert_eq!(count(&connection, "pod0_activity_facts"), 3);
}

#[test]
fn every_fault_seam_rolls_back_state_facts_outboxes_and_receipt() {
    let points = [
        CommitFaultPoint::BeforeMutation,
        CommitFaultPoint::AfterMutation,
        CommitFaultPoint::AfterFacts,
        CommitFaultPoint::AfterEffectIntents,
        CommitFaultPoint::AfterInternalCommands,
        CommitFaultPoint::AfterReceipt,
    ];
    for (index, target) in points.into_iter().enumerate() {
        let fixture = Fixture::new();
        fixture
            .migrate_to_current(30 + u64::try_from(index).unwrap())
            .unwrap();
        let connection = Connection::open(&fixture.store).unwrap();
        connection
            .execute("CREATE TABLE test_state(value TEXT)", [])
            .unwrap();
        drop(connection);
        let result = TransitionCommit::open(&fixture.store)
            .unwrap()
            .commit_with_fault(
                ingress(1),
                plan(),
                UnixTimestampMilliseconds::new(100),
                |transaction, _, value| {
                    transaction
                        .execute("INSERT INTO test_state VALUES(?1)", [value])
                        .unwrap();
                    Ok(StateRevision::new(10))
                },
                |point| {
                    (point != target)
                        .then_some(())
                        .ok_or(StorageError::Interrupted)
                },
            );
        assert!(matches!(result, Err(StorageError::Interrupted)));
        let connection = Connection::open(&fixture.store).unwrap();
        for table in [
            "test_state",
            "pod0_activity_facts",
            "pod0_effect_intents",
            "pod0_internal_command_intents",
            "pod0_transition_receipts",
        ] {
            assert_eq!(count(&connection, table), 0, "{target:?}: {table}");
        }
    }
}

#[path = "transition_commit_causation_tests.rs"]
mod causation;
