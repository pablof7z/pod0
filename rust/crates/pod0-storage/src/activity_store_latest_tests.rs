use std::time::{Duration, Instant};

use pod0_application::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    RequestDisposition,
};
use pod0_domain::{
    ActivityCorrelationId, ActivityId, ActivityTransactionId, CommandId, EpisodeId,
};
use rusqlite::{Connection, params};

use crate::recovery_test_support::Fixture;
use crate::activity_store_latest::{LATEST_PAGE_SQL, LINKED_MAX_SQL};
use crate::{ActivityStore, MAX_ACTIVITY_PAGE_ITEMS, StorageError};

#[test]
fn newest_snapshot_is_episode_scoped_and_stable_across_concurrent_appends() {
    let fixture = Fixture::new();
    fixture.migrate_to_current(219).unwrap();
    let episode = EpisodeId::from_parts(219, 1);
    let unrelated = EpisodeId::from_parts(219, 2);
    seed(&fixture.store, unrelated, 1, 3);
    seed(&fixture.store, episode, 4, 6);
    let store = ActivityStore::open(&fixture.store).unwrap();

    let first = store.latest_page_for_episode(episode, None, None, 2).unwrap();
    assert_eq!(sequences(&first.items), [6, 5]);
    assert_eq!(first.snapshot_through_sequence, Some(6));
    seed(&fixture.store, unrelated, 7, 8);
    seed(&fixture.store, episode, 9, 10);

    let second = store
        .latest_page_for_episode(
            episode,
            first.snapshot_through_sequence,
            first.next_before_sequence,
            2,
        )
        .unwrap();
    assert_eq!(sequences(&second.items), [4]);
    assert_eq!(second.snapshot_through_sequence, Some(6));
    assert_eq!(second.next_before_sequence, None);
    let fresh = store.latest_page_for_episode(episode, None, None, 2).unwrap();
    assert_eq!(sequences(&fresh.items), [10, 9]);
}

#[test]
fn cursors_fail_closed_and_requested_size_is_clamped() {
    let fixture = Fixture::new();
    fixture.migrate_to_current(220).unwrap();
    let episode = EpisodeId::from_parts(220, 1);
    seed(&fixture.store, episode, 1, 205);
    let store = ActivityStore::open(&fixture.store).unwrap();

    assert_eq!(
        store.latest_page_for_episode(episode, Some(205), None, 20),
        Err(StorageError::InvalidActivity)
    );
    assert_eq!(
        store.latest_page_for_episode(episode, Some(206), Some(205), 20),
        Err(StorageError::InvalidActivity)
    );
    assert_eq!(
        store.latest_page_for_episode(episode, Some(205), Some(206), 20),
        Err(StorageError::InvalidActivity)
    );
    assert_eq!(
        store.latest_page_for_episode(episode, Some(205), Some(0), 20),
        Err(StorageError::InvalidActivity)
    );
    assert_eq!(
        store
            .latest_page_for_episode(episode, None, None, u16::MAX)
            .unwrap()
            .items
            .len(),
        usize::from(MAX_ACTIVITY_PAGE_ITEMS)
    );
    assert_eq!(
        store
            .latest_page_for_episode(episode, None, None, 0)
            .unwrap()
            .items
            .len(),
        1
    );
}

#[test]
fn ten_thousand_fact_latest_page_stays_bounded_and_within_budget() {
    let fixture = Fixture::new();
    fixture.migrate_to_current(221).unwrap();
    let episode = EpisodeId::from_parts(221, 1);
    seed(&fixture.store, episode, 1, 10_000);
    let store = ActivityStore::open(&fixture.store).unwrap();

    for _ in 0..3 {
        assert_eq!(latest(&store, episode).items.len(), usize::from(MAX_ACTIVITY_PAGE_ITEMS));
    }
    let mut samples = Vec::with_capacity(20);
    for _ in 0..20 {
        let started = Instant::now();
        let page = latest(&store, episode);
        samples.push(started.elapsed());
        assert_eq!(page.items.len(), usize::from(MAX_ACTIVITY_PAGE_ITEMS));
        assert_eq!(page.items.first().unwrap().sequence, 10_000);
    }
    samples.sort_unstable();
    let median = samples[10];
    let p95 = samples[18];
    eprintln!("activity_latest_10k_median={median:?} p95={p95:?}");
    let p95_budget = if cfg!(debug_assertions) {
        Duration::from_secs(5)
    } else {
        Duration::from_millis(250)
    };
    assert!(p95 < p95_budget, "latest page p95 {p95:?} exceeded {p95_budget:?}");
}

fn latest(store: &ActivityStore, episode: EpisodeId) -> crate::LatestActivityPage {
    store
        .latest_page_for_episode(episode, None, None, MAX_ACTIVITY_PAGE_ITEMS)
        .unwrap()
}

#[test]
fn newest_queries_use_episode_sequence_and_causal_indexes() {
    let fixture = Fixture::new();
    fixture.migrate_to_current(223).unwrap();
    let connection = Connection::open(&fixture.store).unwrap();
    let episode = EpisodeId::from_parts(223, 1).into_bytes();

    let max_plan = explain(&connection, LINKED_MAX_SQL, &[&episode.as_slice()]);
    let page_plan = explain(
        &connection,
        LATEST_PAGE_SQL,
        &[&episode.as_slice(), &100_i64, &101_i64, &201_i64],
    );
    for plan in [&max_plan, &page_plan] {
        assert!(plan.contains("pod0_activity_facts_episode_sequence_v1"), "{plan}");
        assert!(plan.contains("sqlite_autoindex_pod0_activity_facts_1"), "{plan}");
    }
}

fn explain(
    connection: &Connection,
    sql: &str,
    values: &[&dyn rusqlite::ToSql],
) -> String {
    let mut statement = connection.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
    statement
        .query_map(values, |row| row.get::<_, String>(3))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
        .join("\n")
}

fn seed(path: &std::path::Path, episode: EpisodeId, first: u64, last: u64) {
    let mut connection = Connection::open(path).unwrap();
    let transaction = connection.transaction().unwrap();
    for sequence in first..=last {
        let draft = draft(sequence, episode);
        transaction.execute(
            "INSERT INTO pod0_activity_facts(
             activity_id,transaction_id,correlation_id,command_id,actor_code,origin_code,
             subject_code,subject_id,episode_id,fact_code,payload_json,committed_at_ms)
             VALUES(?1,?2,?3,?4,1,1,2,?5,?5,1,?6,?7)",
            params![draft.activity_id.into_bytes(), draft.transaction_id.into_bytes(),
                draft.correlation_id.into_bytes(), draft.command_id.unwrap().into_bytes(),
                episode.into_bytes(), serde_json::to_string(&draft).unwrap(),
                i64::try_from(sequence).unwrap()],
        ).unwrap();
    }
    transaction.commit().unwrap();
}

fn draft(value: u64, episode_id: EpisodeId) -> ActivityFactDraft {
    ActivityFactDraft {
        activity_id: ActivityId::from_parts(219, value),
        transaction_id: ActivityTransactionId::from_parts(219, value),
        correlation_id: ActivityCorrelationId::from_parts(219, value),
        caused_by_activity_id: None,
        command_id: Some(CommandId::from_parts(219, value)),
        host_request_id: None,
        actor: ActivityActor::User,
        origin: ActivityOrigin::UserInterface,
        subject: ActivitySubject::Episode { episode_id },
        episode_id: Some(episode_id),
        fact: ActivityFact::RequestDisposition { disposition: RequestDisposition::Accepted },
    }
}

fn sequences(items: &[pod0_application::CommittedActivityFact]) -> Vec<u64> {
    items.iter().map(|item| item.sequence).collect()
}
