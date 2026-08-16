use pod0_domain::CommandId;
use rusqlite::{Connection, params};

use crate::migration_tests::Fixture;

#[test]
fn schema_39_preserves_effect_leases_and_raises_exact_request_limit() {
    let fixture = Fixture::new();
    fixture.migrate_to(38, 1).unwrap();
    let connection = Connection::open(&fixture.store).unwrap();
    connection.execute("PRAGMA foreign_keys=ON", []).unwrap();
    insert_effect(&connection, 1, 2, 1, None);
    insert_effect(
        &connection,
        2,
        3,
        3,
        Some((1_020, r#"{"outcome":"ok"}"#, r#"{"observation":"kept"}"#)),
    );
    drop(connection);

    fixture.migrate_to(39, 2).unwrap();
    let connection = Connection::open(&fixture.store).unwrap();
    connection.execute("PRAGMA foreign_keys=ON", []).unwrap();
    let preserved: Vec<(i64, i64, Option<String>)> = connection
        .prepare(
            "SELECT i.fence,a.state_code,a.observation_json FROM pod0_effect_intents i \
             JOIN pod0_effect_attempts a ON a.intent_id=i.intent_id ORDER BY i.fence",
        )
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        preserved,
        vec![(2, 1, None), (3, 3, Some(r#"{"observation":"kept"}"#.into()))]
    );
    assert!(connection
        .query_row("PRAGMA foreign_key_check", [], |_| Ok(()))
        .is_err());

    let accepted = "x".repeat(4_097);
    connection
        .execute(
            "UPDATE pod0_effect_intents SET request_json=?1 WHERE fence=2",
            [accepted],
        )
        .unwrap();
    let rejected = "x".repeat(67_108_865);
    assert!(connection
        .execute(
            "UPDATE pod0_effect_intents SET request_json=?1 WHERE fence=2",
            [rejected],
        )
        .is_err());
}

fn insert_effect(
    connection: &Connection,
    ordinal: u64,
    fence: i64,
    state: i64,
    observed: Option<(i64, &str, &str)>,
) {
    let fact = bytes(ordinal * 10 + 1);
    let intent = bytes(ordinal * 10 + 2);
    let correlation = bytes(ordinal * 10 + 3);
    connection.execute(
        "INSERT INTO pod0_activity_facts(
            activity_id,transaction_id,correlation_id,authorized_effect_intent_id,
            actor_code,origin_code,subject_code,subject_id,fact_code,payload_json,committed_at_ms
         ) VALUES(?1,?2,?3,?4,1,1,1,?5,4,'{}',1000)",
        params![fact, bytes(ordinal * 10 + 4), correlation, intent, bytes(ordinal)],
    ).unwrap();
    connection.execute(
        "INSERT INTO pod0_effect_intents(
            intent_id,authorizing_activity_id,correlation_id,effect_kind_code,subject_code,
            subject_id,request_json,state_code,fence,available_at_ms,committed_at_ms
         ) VALUES(?1,?2,?3,3,1,?4,'{}',2,?5,1000,1000)",
        params![intent, fact, correlation, bytes(ordinal), fence],
    ).unwrap();
    let (observed_at, outcome, observation) = observed
        .map(|value| (Some(value.0), Some(value.1), Some(value.2)))
        .unwrap_or((None, None, None));
    connection.execute(
        "INSERT INTO pod0_effect_attempts(
            attempt_id,intent_id,lease_id,fence,state_code,claimed_at_ms,lease_expires_at_ms,
            observed_at_ms,outcome_schema_version,outcome_json,observation_schema_version,
            observation_json
         ) VALUES(?1,?2,?3,?4,?5,1000,1100,?6,?7,?8,?9,?10)",
        params![bytes(ordinal * 10 + 5), intent, bytes(ordinal * 10 + 6), fence, state,
            observed_at, observed_at.map(|_| 1), outcome, observed_at.map(|_| 1), observation],
    ).unwrap();
}

fn bytes(value: u64) -> [u8; 16] {
    CommandId::from_parts(0, value).into_bytes()
}
