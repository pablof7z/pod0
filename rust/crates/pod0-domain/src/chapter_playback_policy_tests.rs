use std::collections::BTreeSet;

use crate::{
    AdSpanEvaluation, AdSpanInput, ChapterAdKind, ChapterArtifact, ChapterArtifactError,
    ChapterArtifactInput, ChapterArtifactProvenance, ChapterArtifactSource, ChapterInput,
    ChapterNavigationDirection, ContentDigest, EpisodeId, PlaybackSeekReason, PodcastId,
    UnixTimestampMilliseconds, decide_automatic_ad_skip, decide_chapter_navigation,
};

#[test]
fn next_navigation_is_strict_at_boundaries_and_ignores_non_toc_items() {
    let artifact = artifact();
    assert_eq!(
        next_target(&artifact, 0),
        Some((252_000, PlaybackSeekReason::NextChapter))
    );
    assert_eq!(
        next_target(&artifact, 252_000),
        Some((1_720_000, PlaybackSeekReason::NextChapter))
    );
    assert_eq!(next_target(&artifact, 9_999_000), None);
}

#[test]
fn previous_navigation_preserves_the_strict_three_second_restart_policy() {
    let artifact = artifact();
    assert_eq!(
        previous_target(&artifact, 352_000),
        (252_000, PlaybackSeekReason::PreviousChapterRestart)
    );
    assert_eq!(
        previous_target(&artifact, 255_000),
        (0, PlaybackSeekReason::PreviousChapter)
    );
    assert_eq!(
        previous_target(&artifact, 255_001),
        (252_000, PlaybackSeekReason::PreviousChapterRestart)
    );
    assert_eq!(
        previous_target(&artifact, 252_000),
        (0, PlaybackSeekReason::PreviousChapter)
    );
}

#[test]
fn previous_navigation_clamps_to_first_before_or_inside_the_first_chapter() {
    let artifact = artifact();
    assert_eq!(
        previous_target(&artifact, 0),
        (0, PlaybackSeekReason::PreviousChapter)
    );
    assert_eq!(
        previous_target(&artifact, 1_000),
        (0, PlaybackSeekReason::PreviousChapter)
    );
}

#[test]
fn automatic_ad_skip_uses_half_open_intervals_for_all_ad_kinds() {
    let artifact = artifact();
    let skipped = BTreeSet::new();
    for (position, target) in [(0, 10_000), (40_000, 50_000), (90_000, 100_000)] {
        let decision = decide_automatic_ad_skip(&artifact, position, true, true, &skipped)
            .expect("ad start should be actionable while playing");
        assert_eq!(decision.target_milliseconds, target);
        assert_eq!(decision.reason, PlaybackSeekReason::AutomaticAdSkip);
        assert!(decision.ad_span_id.is_some());
    }
    assert!(decide_automatic_ad_skip(&artifact, 10_000, true, true, &skipped).is_none());
    assert!(decide_automatic_ad_skip(&artifact, 50_000, true, true, &skipped).is_none());
    assert!(decide_automatic_ad_skip(&artifact, 100_000, true, true, &skipped).is_none());
}

#[test]
fn automatic_ad_skip_respects_setting_play_state_and_stable_skip_identity() {
    let artifact = artifact();
    let first = decide_automatic_ad_skip(&artifact, 1_000, true, true, &BTreeSet::new()).unwrap();
    assert!(decide_automatic_ad_skip(&artifact, 1_000, false, true, &BTreeSet::new()).is_none());
    assert!(decide_automatic_ad_skip(&artifact, 1_000, true, false, &BTreeSet::new()).is_none());
    let skipped = BTreeSet::from([first.ad_span_id.unwrap()]);
    assert!(decide_automatic_ad_skip(&artifact, 1_000, true, true, &skipped).is_none());
}

#[test]
fn overlapping_ad_input_is_rejected_before_policy_evaluation() {
    let mut input = artifact_input();
    input.ad_spans[1].start_milliseconds = 9_999;
    assert_eq!(
        ChapterArtifact::seal(input),
        Err(ChapterArtifactError::AdSpansOverlap)
    );
}

fn next_target(artifact: &ChapterArtifact, position: u64) -> Option<(u64, PlaybackSeekReason)> {
    decide_chapter_navigation(artifact, position, ChapterNavigationDirection::Next)
        .map(|decision| (decision.target_milliseconds, decision.reason))
}

fn previous_target(artifact: &ChapterArtifact, position: u64) -> (u64, PlaybackSeekReason) {
    let decision =
        decide_chapter_navigation(artifact, position, ChapterNavigationDirection::Previous)
            .unwrap();
    (decision.target_milliseconds, decision.reason)
}

fn artifact() -> ChapterArtifact {
    ChapterArtifact::seal(artifact_input()).unwrap()
}

fn artifact_input() -> ChapterArtifactInput {
    ChapterArtifactInput {
        episode_id: EpisodeId::from_parts(1, 1),
        podcast_id: PodcastId::from_parts(2, 2),
        source_revision: "chapter-policy-fixture-v1".to_owned(),
        provenance: ChapterArtifactProvenance {
            source: ChapterArtifactSource::Publisher,
            provider: None,
            model: None,
            policy_version: 0,
            source_payload_digest: ContentDigest::from_bytes([3; 32]),
            transcript_version_id: None,
            transcript_content_digest: None,
            legacy_import: None,
        },
        generated_at: UnixTimestampMilliseconds::new(1_800_000_000_000),
        duration_milliseconds: Some(2_000_000),
        chapters: vec![
            chapter(0, "Cold open", true),
            chapter(100_000, "Ad marker", false),
            chapter(252_000, "Why ketones matter", true),
            chapter(1_720_000, "The Inuit objection", true),
        ],
        ad_span_evaluation: AdSpanEvaluation::Evaluated,
        ad_spans: vec![
            ad(0, 10_000, ChapterAdKind::Preroll),
            ad(40_000, 50_000, ChapterAdKind::Midroll),
            ad(90_000, 100_000, ChapterAdKind::Postroll),
        ],
    }
}

fn chapter(start: u64, title: &str, toc: bool) -> ChapterInput {
    ChapterInput {
        start_milliseconds: start,
        end_milliseconds: None,
        title: title.to_owned(),
        summary: None,
        image_url: None,
        link_url: None,
        include_in_table_of_contents: toc,
        source_episode_id: None,
    }
}

fn ad(start: u64, end: u64, kind: ChapterAdKind) -> AdSpanInput {
    AdSpanInput {
        start_milliseconds: start,
        end_milliseconds: end,
        kind,
    }
}
