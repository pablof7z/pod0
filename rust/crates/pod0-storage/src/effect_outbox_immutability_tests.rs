use pod0_domain::{CommandId, UnixTimestampMilliseconds};
use rusqlite::Connection;

use crate::effect_outbox_tests::commit_effect;
use crate::recovery_test_support::Fixture;
use crate::{EffectOutbox, StorageError};

#[test]
fn database_keeps_effect_requests_immutable_while_leases_advance_state() {
    let fixture = Fixture::new();
    fixture.migrate_to_current(53).unwrap();
    let intent_id = commit_effect(&fixture.store);
    let connection = Connection::open(&fixture.store).unwrap();
    assert!(
        connection
            .execute(
                "UPDATE pod0_effect_intents SET request_json=request_json WHERE intent_id=?1",
                [intent_id.into_bytes().as_slice()],
            )
            .is_err()
    );
    assert!(
        connection
            .execute(
                "DELETE FROM pod0_effect_intents WHERE intent_id=?1",
                [intent_id.into_bytes().as_slice()],
            )
            .is_err()
    );
    drop(connection);

    let lease = EffectOutbox::open(&fixture.store)
        .unwrap()
        .claim_next_generated(UnixTimestampMilliseconds::new(1_000), 1_000)
        .unwrap()
        .unwrap();
    assert_eq!(lease.intent_id, intent_id);
    assert_eq!(lease.fence, 1);

    let connection = Connection::open(&fixture.store).unwrap();
    connection
        .execute("DROP TRIGGER pod0_effect_intents_request_immutable", [])
        .unwrap();
    assert!(matches!(
        crate::schema::validate_schema(&connection, crate::CURRENT_SCHEMA_VERSION),
        Err(StorageError::CorruptSchema { .. })
    ));
    drop(connection);

    assert!(matches!(
        crate::CoreStoreMigrator::new(crate::recovery_test_support::FixedClock).migrate(
            &fixture.store,
            crate::CURRENT_SCHEMA_VERSION,
            &fixture.backup,
            CommandId::from_parts(53, 2),
        ),
        Err(StorageError::CorruptSchema { .. })
    ));
}
