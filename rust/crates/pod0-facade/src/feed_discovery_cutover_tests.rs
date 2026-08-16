use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;

#[test]
fn cutover_without_a_store_reports_storage_unavailable() {
    let facade = Pod0Facade::new();
    let report = facade.feed_discovery_cutover();
    assert_eq!(report.stage, LegacyFeedDiscoveryCutoverStage::Blocked);
    assert_eq!(
        report.failure.map(|failure| failure.code),
        Some(LegacyFeedDiscoveryCutoverFailureCode::StorageUnavailable)
    );
}

#[test]
fn typed_feed_discovery_cutover_preserves_ambiguous_delivery_without_redelivery() {
    let fixture = PlaybackFixture::new();
    let backup_digest = ContentDigest::from_bytes([8; 32]);
    let candidate = LegacyFeedDiscoveryCandidateInput {
        source_occurrence_id: CommandId::from_parts(88, 1),
        podcast_id: fixture.podcast_id,
        episode_id: fixture.episode_id,
        kind: LegacyFeedDiscoveryEffectKindInput::Notification,
        disposition: LegacyFeedDiscoveryDispositionInput::Ambiguous { attempt: 1 },
        observed_at: UnixTimestampMilliseconds::new(1_800_000_000_000),
        expires_at: UnixTimestampMilliseconds::new(1_900_000_000_000),
        published_at: UnixTimestampMilliseconds::new(1_700_000_000_000),
        input_version: "a".repeat(64),
    };
    let inspected = fixture.facade.inspect_legacy_feed_discovery_cutover(
        backup_digest,
        512,
        true,
        1,
        0,
        vec![candidate.clone()],
    );
    assert_eq!(inspected.stage, LegacyFeedDiscoveryCutoverStage::NotStarted);
    assert_eq!(inspected.ambiguous_count, 1);
    let generation = inspected.source_generation.unwrap();

    let staged = fixture.facade.stage_legacy_feed_discovery_cutover(
        backup_digest,
        512,
        true,
        1,
        0,
        vec![candidate],
    );
    assert_eq!(staged.stage, LegacyFeedDiscoveryCutoverStage::Staged);
    assert_eq!(staged.source_generation, Some(generation));
    assert!(fixture.facade.next_leased_host_requests(10).is_empty());

    let committed = fixture
        .facade
        .commit_legacy_feed_discovery_cutover(generation);
    assert_eq!(
        committed.stage,
        LegacyFeedDiscoveryCutoverStage::Authoritative
    );
    assert_eq!(committed.ambiguous_count, 1);
    assert!(fixture.facade.next_leased_host_requests(10).is_empty());
    assert_eq!(fixture.facade.feed_discovery_cutover(), committed);
}
