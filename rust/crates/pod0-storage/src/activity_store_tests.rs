use pod0_application::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    DurableExternalEffectRequest, DurableInternalCommandRequest, NonEmptyActivityFacts,
    RequestDisposition, TransitionPlan,
};
use pod0_domain::{
    ActivityCorrelationId, ActivityId, ActivityTransactionId, CommandId, ContentDigest, EpisodeId,
    StateRevision, UnixTimestampMilliseconds,
};
use rusqlite::{Connection, params};

use crate::recovery_test_support::Fixture;
use crate::{
    ActivityStore, StorageError, TransitionCommit, TransitionIngress, TransitionIngressKind,
};

fn draft(
    activity: u64,
    transaction: u64,
    correlation: u64,
    episode_id: EpisodeId,
) -> ActivityFactDraft {
    ActivityFactDraft {
        activity_id: ActivityId::from_parts(1, activity),
        transaction_id: ActivityTransactionId::from_parts(2, transaction),
        correlation_id: ActivityCorrelationId::from_parts(3, correlation),
        caused_by_activity_id: None,
        command_id: Some(CommandId::from_parts(4, transaction)),
        host_request_id: None,
        actor: ActivityActor::User,
        origin: ActivityOrigin::UserInterface,
        subject: ActivitySubject::Episode { episode_id },
        episode_id: Some(episode_id),
        fact: ActivityFact::RequestDisposition {
            disposition: RequestDisposition::Accepted,
        },
    }
}

fn append(
    path: &std::path::Path,
    ingress_value: u64,
    value: ActivityFactDraft,
    at: i64,
) -> Result<(), StorageError> {
    let plan = TransitionPlan::new(
        value.transaction_id,
        StateRevision::INITIAL,
        (),
        NonEmptyActivityFacts::new(value),
        Vec::<pod0_application::AuthorizedExternalEffect<DurableExternalEffectRequest>>::new(),
        Vec::<pod0_application::AuthorizedInternalCommand<DurableInternalCommandRequest>>::new(),
    )
    .unwrap();
    TransitionCommit::open(path)?.commit_no_state_change(
        TransitionIngress {
            kind: TransitionIngressKind::ApplicationCommand,
            id: CommandId::from_parts(90, ingress_value).into_bytes(),
            fingerprint: ContentDigest::from_bytes([u8::try_from(ingress_value).unwrap(); 32]),
        },
        plan,
        UnixTimestampMilliseconds::new(at),
    )?;
    Ok(())
}

#[test]
fn journal_is_ordered_persistent_and_stably_paginated() {
    let fixture = Fixture::new();
    fixture.migrate_to_current(1).unwrap();
    let store = ActivityStore::open(&fixture.store).unwrap();
    let episode = EpisodeId::from_parts(10, 20);
    for value in 1..=3 {
        append(
            &fixture.store,
            value,
            draft(value, value, 7, episode),
            100 + i64::try_from(value).unwrap(),
        )
        .unwrap();
    }

    let first = store.page_for_episode(episode, None, 2).unwrap();
    assert_eq!(
        first
            .items
            .iter()
            .map(|item| item.sequence)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(first.next_after_sequence, Some(2));

    let reopened = ActivityStore::open(&fixture.store).unwrap();
    let second = reopened
        .page_for_episode(episode, first.next_after_sequence, 2)
        .unwrap();
    assert_eq!(second.items[0].sequence, 3);
    assert_eq!(second.next_after_sequence, None);
    assert_eq!(
        second.items[0].draft.activity_id,
        ActivityId::from_parts(1, 3)
    );
}

#[test]
fn database_rejects_update_delete_and_duplicate_identity() {
    let fixture = Fixture::new();
    fixture.migrate_to_current(2).unwrap();
    let episode = EpisodeId::from_parts(10, 21);
    let facts = NonEmptyActivityFacts::new(draft(1, 1, 1, episode));
    append(&fixture.store, 1, *facts.get(0).unwrap(), 100).unwrap();

    let connection = Connection::open(&fixture.store).unwrap();
    assert!(
        connection
            .execute("UPDATE pod0_activity_facts SET committed_at_ms=101", [])
            .is_err()
    );
    assert!(
        connection
            .execute("DELETE FROM pod0_activity_facts", [])
            .is_err()
    );
    assert!(append(&fixture.store, 2, *facts.get(0).unwrap(), 102).is_err());
}

#[test]
fn causation_must_reference_an_existing_fact_in_the_same_correlation() {
    let fixture = Fixture::new();
    fixture.migrate_to_current(3).unwrap();
    let episode = EpisodeId::from_parts(10, 22);
    let parent = draft(1, 1, 8, episode);
    append(&fixture.store, 1, parent, 100).unwrap();

    let mut mismatched = draft(2, 2, 9, episode);
    mismatched.caused_by_activity_id = Some(parent.activity_id);
    let error = append(&fixture.store, 2, mismatched, 101).unwrap_err();
    assert!(matches!(error, StorageError::Sqlite { .. }));

    let connection = Connection::open(&fixture.store).unwrap();
    connection
        .pragma_update(None, "foreign_keys", true)
        .unwrap();
    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pod0_activity_facts WHERE activity_id=?1",
            params![mismatched.activity_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn episode_timeline_includes_only_direct_facts_and_their_causal_ancestors() {
    let fixture = Fixture::new();
    fixture.migrate_to_current(5).unwrap();
    let episode = EpisodeId::from_parts(10, 23);
    let mut parent = draft(1, 1, 10, episode);
    parent.subject = ActivitySubject::Global;
    parent.episode_id = None;
    append(&fixture.store, 1, parent, 100).unwrap();
    let mut child = draft(2, 2, 10, episode);
    child.caused_by_activity_id = Some(parent.activity_id);
    append(&fixture.store, 2, child, 101).unwrap();
    let unrelated_episode = EpisodeId::from_parts(10, 24);
    append(&fixture.store, 3, draft(3, 3, 11, unrelated_episode), 102).unwrap();

    let page = ActivityStore::open(&fixture.store)
        .unwrap()
        .page_for_episode(episode, None, 20)
        .unwrap();
    assert_eq!(
        page.items
            .iter()
            .map(|item| item.draft.activity_id)
            .collect::<Vec<_>>(),
        vec![parent.activity_id, child.activity_id]
    );
}

#[test]
fn schema_validator_rejects_removed_append_only_protection() {
    let fixture = Fixture::new();
    fixture.migrate_to_current(4).unwrap();
    let connection = Connection::open(&fixture.store).unwrap();
    connection
        .execute("DROP TRIGGER pod0_activity_facts_no_delete", [])
        .unwrap();
    assert!(matches!(
        ActivityStore::open(&fixture.store),
        Err(StorageError::CorruptSchema { .. })
    ));
}
