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
    fixture.migrate_to_current(41).unwrap();
    let committer = TransitionCommit::open(&fixture.store).unwrap();
    committer
        .commit_with(
            ingress(1),
            plan(),
            UnixTimestampMilliseconds::new(100),
            |_, _, _| Ok(StateRevision::new(10)),
        )
        .unwrap();
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
    committer
        .commit_no_state_change(
            target_ingress,
            target_plan(true),
            UnixTimestampMilliseconds::new(102),
        )
        .unwrap();
    let connection = Connection::open(&fixture.store).unwrap();
    let state: i64 = connection
        .query_row(
            "SELECT state_code FROM pod0_internal_command_intents",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(state, 2);
}
