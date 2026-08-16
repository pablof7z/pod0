use pod0_domain::CommandId;
use rusqlite::{Connection, params};

use crate::migration_tests::Fixture;

#[test]
fn schema_40_fences_only_active_legacy_domain_effects_and_keeps_terminal_history() {
    let fixture = Fixture::new();
    fixture.migrate_to(39, 1).unwrap();
    let connection = Connection::open(&fixture.store).unwrap();
    insert_legacy(&connection, 1, 1, None);
    insert_legacy(&connection, 2, 2, Some(1));
    insert_legacy(&connection, 3, 3, Some(3));
    insert_exact(&connection, 4);
    drop(connection);

    fixture.migrate_to(40, 2).unwrap();
    let connection = Connection::open(&fixture.store).unwrap();
    assert_eq!(states(&connection), vec![(1, 4), (2, 4), (3, 3), (4, 1)]);
    assert_eq!(attempt_states(&connection), vec![(2, 4), (3, 3)]);
    assert_eq!(recovery_count(&connection), 2);
    assert_eq!(recovery_fact_count(&connection), 2);
    drop(connection);

    let connection = Connection::open(&fixture.store).unwrap();
    let pending: Vec<Vec<u8>> = connection
        .prepare("SELECT intent_id FROM pod0_effect_intents WHERE state_code=1")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(pending, vec![bytes(42)]);
}

#[test]
fn current_decoder_rejects_missing_execution_and_decodes_named_legacy_only() {
    let missing = serde_json::from_str::<pod0_application::DurableExternalEffectRequest>(
        r#"{"kind":"CoreWake","subject":"Global","episode_id":null,"not_before":null,"deadline_at":null}"#,
    );
    assert!(missing.is_err());
    let legacy = request_json("DomainDerived");
    assert!(matches!(
        serde_json::from_str::<pod0_application::DurableExternalEffectRequest>(&legacy)
            .unwrap()
            .execution,
        pod0_application::DurableEffectExecution::LegacyDomainDerived
    ));
}

fn insert_legacy(connection: &Connection, ordinal: u64, state: i64, attempt: Option<i64>) {
    insert(connection, ordinal, state, &request_json("DomainDerived"));
    if let Some(attempt_state) = attempt {
        connection.execute(
            "INSERT INTO pod0_effect_attempts(attempt_id,intent_id,lease_id,fence,state_code,
             claimed_at_ms,lease_expires_at_ms) VALUES(?1,?2,?3,1,?4,1000,1100)",
            params![bytes(ordinal * 10 + 5), bytes(ordinal * 10 + 2),
                bytes(ordinal * 10 + 6), attempt_state],
        ).unwrap();
    }
}

fn insert_exact(connection: &Connection, ordinal: u64) {
    let execution = serde_json::json!({"Lifecycle":{"request":{
        "request_id": bytes_json(ordinal * 10 + 7),
        "command_id": bytes_json(ordinal * 10 + 8),
        "cancellation_id": bytes_json(ordinal * 10 + 9),
        "issued_revision":{"value":0},"wake_at":{"value":1000},
        "reason":{"Unsupported":{"wire_code":1}},"attempt":1
    }}});
    insert(connection, ordinal, 1, &base_request(execution));
}

fn insert(connection: &Connection, ordinal: u64, state: i64, request: &str) {
    let fact = bytes(ordinal * 10 + 1);
    let intent = bytes(ordinal * 10 + 2);
    let correlation = bytes(ordinal * 10 + 3);
    connection.execute(
        "INSERT INTO pod0_activity_facts(activity_id,transaction_id,correlation_id,
         authorized_effect_intent_id,actor_code,origin_code,subject_code,subject_id,fact_code,
         payload_json,committed_at_ms) VALUES(?1,?2,?3,?4,1,1,1,?5,4,'{}',1000)",
        params![fact, bytes(ordinal * 10 + 4), correlation, intent, bytes(ordinal)],
    ).unwrap();
    connection.execute(
        "INSERT INTO pod0_effect_intents(intent_id,authorizing_activity_id,correlation_id,
         effect_kind_code,subject_code,subject_id,request_json,state_code,fence,available_at_ms,
         committed_at_ms) VALUES(?1,?2,?3,10,1,?4,?5,?6,0,1000,1000)",
        params![intent, fact, correlation, bytes(ordinal), request, state],
    ).unwrap();
}

fn request_json(execution: &str) -> String {
    base_request(serde_json::Value::String(execution.to_owned()))
}

fn base_request(execution: serde_json::Value) -> String {
    serde_json::json!({"kind":"CoreWake","subject":"Global","episode_id":null,
        "not_before":null,"deadline_at":null,"execution":execution}).to_string()
}

fn bytes_json(value: u64) -> serde_json::Value {
    serde_json::to_value(CommandId::from_parts(0, value)).unwrap()
}

fn states(connection: &Connection) -> Vec<(u8, i64)> {
    rows(connection, "SELECT rowid,state_code FROM pod0_effect_intents ORDER BY rowid")
}

fn attempt_states(connection: &Connection) -> Vec<(u8, i64)> {
    rows(connection, "SELECT rowid+1,state_code FROM pod0_effect_attempts ORDER BY rowid")
}

fn rows(connection: &Connection, sql: &str) -> Vec<(u8, i64)> {
    connection.prepare(sql).unwrap().query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap().map(Result::unwrap).collect()
}

fn recovery_count(connection: &Connection) -> i64 {
    connection.query_row("SELECT COUNT(*) FROM pod0_legacy_effect_recovery_v40", [], |row| row.get(0)).unwrap()
}

fn recovery_fact_count(connection: &Connection) -> i64 {
    connection.query_row(
        "SELECT COUNT(*) FROM pod0_legacy_effect_recovery_v40 recovery
         JOIN pod0_activity_facts fact ON fact.activity_id=recovery.recovery_activity_id
         WHERE fact.fact_code=7",
        [],
        |row| row.get(0),
    ).unwrap()
}

fn bytes(value: u64) -> [u8; 16] { CommandId::from_parts(0, value).into_bytes() }
