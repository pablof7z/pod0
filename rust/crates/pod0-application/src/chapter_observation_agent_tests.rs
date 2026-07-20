use pod0_domain::{ChapterArtifactSource, ContentDigest, EpisodeId};

use crate::chapter_observation_test_support::{agent, qualified};
use crate::{
    AgentComposedChapterObservation, ChapterObservationProjection, ChapterObservationRejection,
    MAX_AGENT_COMPOSED_CHAPTER_ITEMS, qualify_agent_composed_chapter_observation,
};

#[test]
fn agent_material_preserves_order_source_links_and_exact_playable_bounds() {
    let (artifact, _) = qualified(qualify_agent_composed_chapter_observation(agent()));

    assert_eq!(
        artifact.provenance.source,
        ChapterArtifactSource::AgentComposed
    );
    assert_eq!(artifact.provenance.provider.as_deref(), Some("elevenlabs"));
    assert_eq!(artifact.chapters.len(), 2);
    assert_eq!(artifact.chapters[0].start_milliseconds, 0);
    assert_eq!(artifact.chapters[0].end_milliseconds, Some(10_250));
    assert_eq!(
        artifact.chapters[0].summary.as_deref(),
        Some("Calm context.")
    );
    assert_eq!(
        artifact.chapters[0].image_url.as_deref(),
        Some("file:///tmp/agent-art.jpg")
    );
    assert_eq!(artifact.chapters[1].start_milliseconds, 10_250);
    assert_eq!(artifact.chapters[1].end_milliseconds, Some(30_000));
    assert_eq!(
        artifact.chapters[1].source_episode_id,
        Some(EpisodeId::from_parts(99, 7))
    );
    assert_eq!(
        artifact.chapters[1].link_url.as_deref(),
        Some("https://example.test/episode?t=42")
    );
}

#[test]
fn agent_rejects_invalid_timestamps_ranges_and_urls() {
    let mut non_finite = agent();
    non_finite.items[0].start_seconds = f64::NAN;
    assert_rejected(non_finite, ChapterObservationRejection::InvalidTimestamp);

    let mut overlap = agent();
    overlap.items[1].start_seconds = 9.0;
    assert_rejected(overlap, ChapterObservationRejection::InvalidRange);

    let mut beyond_duration = agent();
    beyond_duration.items[1].end_seconds = 31.0;
    assert_rejected(beyond_duration, ChapterObservationRejection::InvalidRange);

    let mut invalid_url = agent();
    invalid_url.items[1].link_url = Some("javascript:alert(1)".into());
    assert_rejected(invalid_url, ChapterObservationRejection::InvalidUrl);
}

#[test]
fn agent_requires_bounded_material_and_exact_provenance() {
    let mut empty = agent();
    empty.items.clear();
    assert_rejected(empty, ChapterObservationRejection::NoUsableChapters);

    let mut too_many = agent();
    too_many.items = vec![too_many.items[0].clone(); MAX_AGENT_COMPOSED_CHAPTER_ITEMS + 1];
    assert_rejected(too_many, ChapterObservationRejection::CollectionLimit);

    let mut missing_digest = agent();
    missing_digest.source_payload_digest = ContentDigest::default();
    assert_rejected(
        missing_digest,
        ChapterObservationRejection::InvalidProvenance,
    );

    let mut future_policy = agent();
    future_policy.policy_version = 2;
    assert_rejected(
        future_policy,
        ChapterObservationRejection::UnsupportedPolicy { policy_version: 2 },
    );

    let mut invalid_revision = agent();
    invalid_revision.composition_revision = " revision-with-leading-space".into();
    assert_rejected(
        invalid_revision,
        ChapterObservationRejection::InvalidProvenance,
    );
}

#[test]
fn agent_identity_is_deterministic_and_material_evidence_changes_generation() {
    let original = agent();
    let mut later = original.clone();
    later.generated_at.value += 1_000;
    let (first, first_fingerprint) =
        qualified(qualify_agent_composed_chapter_observation(original));
    let (second, second_fingerprint) = qualified(qualify_agent_composed_chapter_observation(later));
    assert_eq!(first.artifact_id, second.artifact_id);
    assert_eq!(first_fingerprint, second_fingerprint);
    assert_ne!(first.integrity_digest, second.integrity_digest);

    let mut changed_source = agent();
    changed_source.items[1].source_episode_id = Some(EpisodeId::from_parts(99, 8));
    let (changed, changed_fingerprint) =
        qualified(qualify_agent_composed_chapter_observation(changed_source));
    assert_ne!(first.artifact_id, changed.artifact_id);
    assert_ne!(first_fingerprint, changed_fingerprint);

    let mut changed_revision = agent();
    changed_revision.composition_revision = "agent-composition-v2".into();
    let (changed_revision, _) =
        qualified(qualify_agent_composed_chapter_observation(changed_revision));
    assert_ne!(first.artifact_id, changed_revision.artifact_id);
}

fn assert_rejected(
    observation: AgentComposedChapterObservation,
    expected: ChapterObservationRejection,
) {
    assert_eq!(
        qualify_agent_composed_chapter_observation(observation),
        ChapterObservationProjection::Rejected { reason: expected }
    );
}
