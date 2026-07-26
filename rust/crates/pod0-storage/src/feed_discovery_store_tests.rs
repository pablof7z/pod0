use pod0_domain::CommandId;
use rusqlite::Connection;

use crate::feed_discovery_store_test_support::*;
use crate::listening_import_test_support::id;
use crate::{LibraryStore, StorageError};

#[test]
fn feed_discovery_commit_is_exact_replayable_and_durable_across_restart() {
    let (fixture, store) = empty_authoritative_store();
    let podcast = podcast(&store);
    let older = episode(podcast.podcast_id, 1, BASE_TIME + 1);
    let newer = episode(podcast.podcast_id, 2, BASE_TIME + 2);
    let episodes = vec![older.clone(), newer.clone()];
    let command = id(100);
    let fingerprint = "a".repeat(64);

    let applied = store
        .apply_feed(
            command,
            &fingerprint,
            podcast.clone(),
            episodes.clone(),
            false,
            true,
            None,
            None,
            BASE_TIME + 10,
        )
        .unwrap();

    assert_eq!(applied.inserted_episode_count, 2);
    let occurrence_id = applied.discovery_occurrence_id.unwrap();
    let pending = store.pending_feed_discoveries(10).unwrap();
    assert_eq!(pending.len(), 1);
    let occurrence = &pending[0];
    assert_eq!(occurrence.occurrence_id, occurrence_id);
    assert_eq!(occurrence.command_id, command);
    assert_eq!(occurrence.podcast_id, podcast.podcast_id);
    assert_eq!(
        occurrence.workflow_schema_version,
        pod0_application::FEED_DISCOVERY_WORKFLOW_SCHEMA_VERSION
    );
    assert_eq!(
        occurrence.policy_version,
        pod0_application::FEED_DISCOVERY_POLICY_VERSION
    );
    assert!(occurrence.is_initial_population);
    assert_eq!(occurrence.observed_at.value, BASE_TIME + 10);
    assert_eq!(
        occurrence
            .items
            .iter()
            .map(|item| item.episode_id)
            .collect::<Vec<_>>(),
        [newer.episode_id, older.episode_id]
    );
    for expected in &episodes {
        let item = occurrence
            .items
            .iter()
            .find(|item| item.episode_id == expected.episode_id)
            .unwrap();
        assert_eq!(
            item.item_id,
            pod0_application::feed_discovery_item_id(occurrence_id, expected.episode_id)
        );
        assert_eq!(
            item.input_version,
            pod0_application::feed_discovery_item_input_version(expected)
        );
    }

    drop(store);
    let reopened = LibraryStore::open_authoritative(&fixture.target).unwrap();
    assert_eq!(reopened.pending_feed_discoveries(10).unwrap(), pending);
    assert_eq!(
        reopened
            .apply_feed(
                command,
                &fingerprint,
                podcast.clone(),
                episodes.clone(),
                false,
                true,
                None,
                None,
                BASE_TIME + 20,
            )
            .unwrap(),
        applied
    );
    assert_eq!(
        reopened.apply_feed(
            command,
            &"b".repeat(64),
            podcast.clone(),
            episodes.clone(),
            false,
            true,
            None,
            None,
            BASE_TIME + 21,
        ),
        Err(StorageError::CommandConflict)
    );

    let original_input_version = pending[0].items[0].input_version.clone();
    let mut retitled = newer;
    retitled.title = "Updated metadata".to_owned();
    let metadata_only = reopened
        .apply_feed(
            id(101),
            &"c".repeat(64),
            podcast,
            vec![retitled],
            false,
            true,
            None,
            None,
            BASE_TIME + 22,
        )
        .unwrap();
    assert_eq!(metadata_only.discovery_occurrence_id, None);
    assert_eq!(metadata_only.inserted_episode_count, 0);
    assert_eq!(
        reopened.pending_feed_discoveries(10).unwrap()[0].items[0].input_version,
        original_input_version
    );
}

#[test]
fn subscribe_empty_and_metadata_only_applies_do_not_create_discoveries() {
    let (_fixture, store) = empty_authoritative_store();
    let podcast = podcast(&store);
    let first = episode(podcast.podcast_id, 10, BASE_TIME);

    let subscribed = store
        .apply_feed(
            id(110),
            &"d".repeat(64),
            podcast.clone(),
            vec![first.clone()],
            true,
            false,
            None,
            None,
            BASE_TIME,
        )
        .unwrap();
    assert_eq!(subscribed.discovery_occurrence_id, None);
    assert_eq!(subscribed.inserted_episode_count, 1);
    assert_eq!(
        store
            .apply_feed(
                id(110),
                &"d".repeat(64),
                podcast.clone(),
                vec![first],
                true,
                false,
                None,
                None,
                BASE_TIME + 1,
            )
            .unwrap(),
        subscribed
    );

    let empty = store
        .apply_feed(
            id(111),
            &"e".repeat(64),
            podcast,
            Vec::new(),
            false,
            true,
            None,
            None,
            BASE_TIME + 2,
        )
        .unwrap();
    assert_eq!(empty.discovery_occurrence_id, None);
    assert_eq!(empty.inserted_episode_count, 0);
    assert!(store.pending_feed_discoveries(10).unwrap().is_empty());
}

#[test]
fn injected_item_failure_rolls_back_episode_occurrence_and_command_receipt() {
    let (fixture, store) = empty_authoritative_store();
    let podcast = podcast(&store);
    Connection::open(&fixture.target)
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER fail_feed_discovery_item
             BEFORE INSERT ON pod0_feed_discovery_items
             BEGIN SELECT RAISE(ABORT,'injected feed discovery failure'); END;",
        )
        .unwrap();

    assert!(matches!(
        store.apply_feed(
            id(120),
            &"f".repeat(64),
            podcast.clone(),
            vec![episode(podcast.podcast_id, 20, BASE_TIME)],
            false,
            true,
            None,
            None,
            BASE_TIME,
        ),
        Err(StorageError::Sqlite {
            operation: "insert feed discovery item"
        })
    ));
    assert!(store.snapshot().unwrap().episodes.is_empty());
    assert!(store.pending_feed_discoveries(10).unwrap().is_empty());
    let connection = Connection::open(&fixture.target).unwrap();
    let receipts: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pod0_library_commands WHERE command_id=?1",
            [id(120).into_bytes().as_slice()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(receipts, 0);
    connection
        .execute("DROP TRIGGER fail_feed_discovery_item", [])
        .unwrap();
    drop(connection);

    assert!(
        store
            .apply_feed(
                id(120),
                &"f".repeat(64),
                podcast.clone(),
                vec![episode(podcast.podcast_id, 20, BASE_TIME)],
                false,
                true,
                None,
                None,
                BASE_TIME,
            )
            .is_ok()
    );
}

#[test]
fn pending_reader_is_bounded_and_deterministically_ordered() {
    let (_fixture, store) = empty_authoritative_store();
    let podcast = podcast(&store);
    for offset in 0_u64..65 {
        store
            .apply_feed(
                CommandId::from_parts(3, offset + 1),
                &format!("{offset:064x}"),
                podcast.clone(),
                vec![episode(
                    podcast.podcast_id,
                    offset + 100,
                    BASE_TIME + i64::try_from(offset).unwrap(),
                )],
                false,
                true,
                None,
                None,
                BASE_TIME + i64::try_from(offset).unwrap(),
            )
            .unwrap();
    }

    let bounded = store.pending_feed_discoveries(u16::MAX).unwrap();
    assert_eq!(bounded.len(), 64);
    assert_eq!(bounded[0].command_id, CommandId::from_parts(3, 1));
    assert_eq!(bounded[63].command_id, CommandId::from_parts(3, 64));
    assert_eq!(store.pending_feed_discoveries(0).unwrap().len(), 1);
}
