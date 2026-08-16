use pod0_domain::EpisodeId;
use rusqlite::{Connection, params};

use crate::activity_store_tests::draft;
use crate::recovery_test_support::Fixture;
use crate::{ActivityStore, StorageError, restore_backup_to_new_store};

#[test]
fn reader_rejects_payload_that_disagrees_with_indexed_identity() {
    let fixture = Fixture::new();
    fixture.migrate_to_current(222).unwrap();
    let episode = EpisodeId::from_parts(222, 1);
    let stored = draft(1, 1, 1, episode);
    let payload = draft(2, 2, 2, episode);
    let connection = Connection::open(&fixture.store).unwrap();
    connection.execute(
        "INSERT INTO pod0_activity_facts(
         activity_id,transaction_id,correlation_id,command_id,actor_code,origin_code,
         subject_code,subject_id,episode_id,fact_code,payload_json,committed_at_ms)
         VALUES(?1,?2,?3,?4,1,1,2,?5,?5,1,?6,100)",
        params![stored.activity_id.into_bytes(), stored.transaction_id.into_bytes(),
            stored.correlation_id.into_bytes(), stored.command_id.unwrap().into_bytes(),
            episode.into_bytes(), serde_json::to_string(&payload).unwrap()],
    ).unwrap();

    assert!(matches!(
        ActivityStore::open(&fixture.store)
            .unwrap()
            .latest_page_for_episode(episode, None, None, 20),
        Err(StorageError::Sqlite { .. })
    ));
}

#[test]
fn schema_backup_and_restore_preserve_exact_journal_order_and_protection() {
    let fixture = Fixture::new();
    fixture.migrator.migrate(
        &fixture.store, 38, &fixture.backup, pod0_domain::CommandId::from_parts(222, 2),
    ).unwrap();
    let episode = EpisodeId::from_parts(222, 2);
    insert_legacy_fact(&fixture.store, draft(1, 1, 1, episode), 100);
    insert_legacy_fact(&fixture.store, draft(2, 2, 1, episode), 101);
    fixture.migrate_to_current(223).unwrap();

    let restored = fixture._directory.path().join("activity-restored.sqlite");
    restore_backup_to_new_store(&fixture.backup, &restored).unwrap();
    fixture.migrator.migrate(
        &restored, crate::CURRENT_SCHEMA_VERSION,
        &fixture._directory.path().join("restore-upgrade.backup.sqlite"),
        pod0_domain::CommandId::from_parts(222, 3),
    ).unwrap();
    let page = ActivityStore::open(&restored).unwrap()
        .latest_page_for_episode(episode, None, None, 20).unwrap();
    assert_eq!(page.items.iter().map(|item| item.sequence).collect::<Vec<_>>(), [2, 1]);
    let connection = Connection::open(restored).unwrap();
    assert!(connection.execute("DELETE FROM pod0_activity_facts", []).is_err());
}

fn insert_legacy_fact(path: &std::path::Path, fact: pod0_application::ActivityFactDraft, at: i64) {
    let connection = Connection::open(path).unwrap();
    connection.execute(
        "INSERT INTO pod0_activity_facts(
         activity_id,transaction_id,correlation_id,command_id,actor_code,origin_code,
         subject_code,subject_id,episode_id,fact_code,payload_json,committed_at_ms)
         VALUES(?1,?2,?3,?4,1,1,2,?5,?5,1,?6,?7)",
        params![fact.activity_id.into_bytes(), fact.transaction_id.into_bytes(),
            fact.correlation_id.into_bytes(), fact.command_id.unwrap().into_bytes(),
            fact.episode_id.unwrap().into_bytes(), serde_json::to_string(&fact).unwrap(), at],
    ).unwrap();
}
