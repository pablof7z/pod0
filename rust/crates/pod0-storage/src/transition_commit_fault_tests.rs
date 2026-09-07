use super::*;

#[test]
fn every_fault_seam_exposes_only_old_or_complete_transition() {
    let points = [
        CommitFaultPoint::BeforeMutation,
        CommitFaultPoint::AfterMutation,
        CommitFaultPoint::AfterFacts,
        CommitFaultPoint::AfterEffectIntents,
        CommitFaultPoint::AfterInternalCommands,
        CommitFaultPoint::AfterReceipt,
        CommitFaultPoint::AfterCommit,
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
        let committed = target == CommitFaultPoint::AfterCommit;
        assert_eq!(count(&connection, "test_state"), i64::from(committed));
        assert_eq!(
            count(&connection, "pod0_activity_facts"),
            if committed { 3 } else { 0 }
        );
        for table in [
            "pod0_effect_intents",
            "pod0_internal_command_intents",
            "pod0_transition_receipts",
        ] {
            assert_eq!(
                count(&connection, table),
                i64::from(committed),
                "{target:?}: {table}"
            );
        }
        drop(connection);

        let replay = TransitionCommit::open(&fixture.store)
            .unwrap()
            .commit_with(
                ingress(1),
                plan(),
                UnixTimestampMilliseconds::new(101),
                |transaction, _, value| {
                    transaction
                        .execute("INSERT INTO test_state VALUES(?1)", [value])
                        .unwrap();
                    Ok(StateRevision::new(10))
                },
            )
            .unwrap();
        assert_eq!(replay.replayed, committed, "{target:?}");
        let connection = Connection::open(&fixture.store).unwrap();
        assert_eq!(count(&connection, "test_state"), 1, "{target:?}");
        assert_eq!(count(&connection, "pod0_activity_facts"), 3, "{target:?}");
        assert_eq!(count(&connection, "pod0_effect_intents"), 1, "{target:?}");
        assert_eq!(
            count(&connection, "pod0_internal_command_intents"),
            1,
            "{target:?}"
        );
        assert_eq!(
            count(&connection, "pod0_transition_receipts"),
            1,
            "{target:?}"
        );
    }
}
