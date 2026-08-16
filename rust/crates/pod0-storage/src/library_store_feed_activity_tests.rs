use super::*;

#[test]
fn unsubscribe_is_atomic_with_its_typed_library_activity() {
    let fixture = imported_fixture();
    commit_listening_cutover(&fixture.target, 1_800_000_000_000).unwrap();
    let store = LibraryStore::open_authoritative(&fixture.target).unwrap();
    let podcast_id = store.snapshot().unwrap().podcasts[0].podcast_id;
    let command_id = id(60);
    let revision = store
        .unsubscribe(command_id, &"6".repeat(64), podcast_id, 1_800_000_000_060)
        .unwrap();
    assert_eq!(
        store
            .unsubscribe(command_id, &"6".repeat(64), podcast_id, 1_800_000_000_061)
            .unwrap(),
        revision
    );
    let correlation = pod0_application::CommandActivityIdentity::new(command_id).correlation_id();
    let activity = crate::ActivityStore::open(&fixture.target)
        .unwrap()
        .page_for_correlation(correlation, None, 20)
        .unwrap();
    assert_eq!(activity.items.len(), 2);
    assert!(activity.items.iter().any(|item| matches!(
        item.draft.fact,
        pod0_application::ActivityFact::DomainTransition {
            kind: pod0_application::DomainTransitionKind::LibraryFeed(
                pod0_application::LibraryFeedTransition::SubscriptionChanged
            ),
            ..
        }
    )));
}

#[test]
fn listening_reset_is_durably_a_global_erasure_not_a_playback_transition() {
    let fixture = imported_fixture();
    commit_listening_cutover(&fixture.target, 1_800_000_000_000).unwrap();
    let store = LibraryStore::open_authoritative(&fixture.target).unwrap();
    let command_id = id(61);
    let revision = store
        .reset_listening_data(command_id, &"7".repeat(64), 1_800_000_000_061)
        .unwrap();
    assert_eq!(
        store
            .reset_listening_data(command_id, &"7".repeat(64), 1_800_000_000_062)
            .unwrap(),
        revision
    );
    let correlation = pod0_application::CommandActivityIdentity::new(command_id).correlation_id();
    let activity = crate::ActivityStore::open(&fixture.target)
        .unwrap()
        .page_for_correlation(correlation, None, 20)
        .unwrap();
    assert_eq!(activity.items.len(), 3);
    assert!(activity.items.iter().any(|item| matches!(
        item.draft.fact,
        pod0_application::ActivityFact::DomainTransition {
            kind: pod0_application::DomainTransitionKind::Lifecycle(
                pod0_application::LifecycleTransition::UserDataErasureChanged
            ),
            ..
        }
    )));
    assert!(activity.items.iter().all(|item| !matches!(
        item.draft.fact,
        pod0_application::ActivityFact::DomainTransition {
            kind: pod0_application::DomainTransitionKind::Playback(_),
            ..
        }
    )));
}

#[test]
fn feed_admission_atomically_authorizes_one_exact_leased_fetch() {
    let fixture = imported_fixture();
    commit_listening_cutover(&fixture.target, 1_800_000_000_000).unwrap();
    let store = LibraryStore::open_authoritative(&fixture.target).unwrap();
    let command_id = id(62);
    let podcast_id = pod0_domain::PodcastId::from_parts(0, 62);
    let outcome = store
        .ensure_feed_fetch_workflow(crate::FeedFetchEnsureInput {
            command_id,
            command_fingerprint: "8".repeat(64),
            cancellation_id: pod0_domain::CancellationId::from_parts(0, 63),
            source_url: "https://durable.test/feed".to_owned(),
            feed_key: "https://durable.test/feed".to_owned(),
            podcast_id,
            placeholder_title: "durable.test".to_owned(),
            intent: crate::StoredFeedFetchIntent::Subscribe,
            entity_tag: None,
            last_modified: None,
            issued_revision: store.snapshot().unwrap().playback.revision,
            now_ms: 1_800_000_000_062,
            deadline_at_ms: 1_800_086_400_062,
        })
        .unwrap();
    assert_eq!(outcome.podcast_id, podcast_id);
    let lease = store
        .claim_next_effect(
            pod0_domain::UnixTimestampMilliseconds::new(1_800_000_000_062),
            120_000,
        )
        .unwrap()
        .expect("feed effect must be durable before delivery");
    assert_eq!(
        lease.request.kind,
        pod0_application::ExternalEffectKind::FeedNetwork
    );
    let pod0_application::DurableEffectExecution::Feed { request } = lease.request.execution else {
        panic!("feed effect must retain exact execution data");
    };
    assert_eq!(request.command_id, command_id);
    assert_eq!(request.podcast_id(), podcast_id);
    let correlation = pod0_application::CommandActivityIdentity::new(command_id).correlation_id();
    let activity = crate::ActivityStore::open(&fixture.target)
        .unwrap()
        .page_for_correlation(correlation, None, 20)
        .unwrap();
    assert_eq!(activity.items.len(), 3);
    assert!(activity.items.iter().any(|item| matches!(
        item.draft.fact,
        pod0_application::ActivityFact::EffectAuthorized {
            kind: pod0_application::ExternalEffectKind::FeedNetwork,
            ..
        }
    )));
}
