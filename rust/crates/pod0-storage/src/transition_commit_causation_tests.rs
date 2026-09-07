use super::*;

#[test]
fn rejected_reason_is_preserved_by_idempotent_replay() {
    let fixture = Fixture::new();
    fixture.migrate_to_current(40).unwrap();
    let fact = draft(
        1,
        ActivityFact::RequestDisposition {
            disposition: RequestDisposition::Rejected {
                reason: RequestRejectionReason::PrivacyBoundary,
            },
        },
    );
    let plan = || {
        TransitionPlan::new(
            fact.transaction_id,
            StateRevision::new(3),
            (),
            NonEmptyActivityFacts::new(fact),
            Vec::new(),
            Vec::new(),
        )
        .unwrap()
    };
    let committer = TransitionCommit::open(&fixture.store).unwrap();
    committer
        .commit_no_state_change(ingress(4), plan(), UnixTimestampMilliseconds::new(100))
        .unwrap();
    let replay = committer
        .commit_no_state_change(ingress(4), plan(), UnixTimestampMilliseconds::new(101))
        .unwrap();
    assert_eq!(
        replay.disposition,
        RequestDisposition::Rejected {
            reason: RequestRejectionReason::PrivacyBoundary,
        }
    );
}

#[test]
fn internal_command_requires_and_atomically_consumes_its_causal_link() {
    let fixture = Fixture::new();
    let store =
        crate::create_authoritative_store(&fixture.store, CommandId::from_parts(40, 1), 100)
            .unwrap();
    let connection = Connection::open(&fixture.store).unwrap();
    connection
        .execute("CREATE TABLE test_target_state(value TEXT)", [])
        .unwrap();
    drop(connection);
    let committer = TransitionCommit::open(&fixture.store).unwrap();
    committer
        .commit_with(
            ingress(1),
            plan(),
            UnixTimestampMilliseconds::new(100),
            |_, _, _| Ok(StateRevision::new(10)),
        )
        .unwrap();
    drop(committer);

    let pending = store.pending_internal_commands(100).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(
        pending[0].internal_command_id,
        InternalCommandId::from_parts(8, 1)
    );
    assert_eq!(
        pending[0].authorizing_activity_id,
        ActivityId::from_parts(1, 3)
    );
    assert_eq!(
        pending[0].correlation_id,
        ActivityCorrelationId::from_parts(3, 1)
    );
    assert_eq!(pending[0].request.target, ActivityDomain::RecallKnowledge);

    let committer = TransitionCommit::open(&fixture.store).unwrap();
    let target_plan = |linked: bool| {
        let episode_id = EpisodeId::from_parts(5, 6);
        let fact = ActivityFactDraft {
            activity_id: ActivityId::from_parts(30, u64::from(linked)),
            transaction_id: ActivityTransactionId::from_parts(31, u64::from(linked)),
            correlation_id: ActivityCorrelationId::from_parts(3, 1),
            caused_by_activity_id: linked.then_some(ActivityId::from_parts(1, 3)),
            command_id: None,
            host_request_id: None,
            actor: ActivityActor::System,
            origin: ActivityOrigin::InternalCommand,
            subject: ActivitySubject::Episode { episode_id },
            episode_id: Some(episode_id),
            fact: ActivityFact::RequestDisposition {
                disposition: RequestDisposition::Accepted,
            },
        };
        TransitionPlan::new(
            fact.transaction_id,
            StateRevision::new(10),
            (),
            NonEmptyActivityFacts::new(fact),
            Vec::new(),
            Vec::new(),
        )
        .unwrap()
    };
    let target_ingress = TransitionIngress {
        kind: TransitionIngressKind::InternalCommand,
        id: InternalCommandId::from_parts(8, 1).into_bytes(),
        fingerprint: ContentDigest::from_bytes([9; 32]),
    };
    assert!(matches!(
        committer.commit_no_state_change(
            target_ingress,
            target_plan(false),
            UnixTimestampMilliseconds::new(101),
        ),
        Err(StorageError::InvalidActivity)
    ));
    let receipt = committer
        .commit_with(
            target_ingress,
            target_plan(true),
            UnixTimestampMilliseconds::new(102),
            |transaction, expected, ()| {
                assert_eq!(expected, StateRevision::new(10));
                transaction
                    .execute("INSERT INTO test_target_state VALUES('applied')", [])
                    .unwrap();
                Ok(StateRevision::new(11))
            },
        )
        .unwrap();
    assert!(!receipt.replayed);
    let replay = committer
        .commit_with(
            target_ingress,
            target_plan(true),
            UnixTimestampMilliseconds::new(103),
            |_, _, _| panic!("replayed delivery must not mutate target state"),
        )
        .unwrap();
    assert!(replay.replayed);
    assert!(store.pending_internal_commands(100).unwrap().is_empty());
    let connection = Connection::open(&fixture.store).unwrap();
    let state: i64 = connection
        .query_row(
            "SELECT state_code FROM pod0_internal_command_intents",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(state, 2);
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM test_target_state", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
}
