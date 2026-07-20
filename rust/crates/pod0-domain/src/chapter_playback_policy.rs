use std::collections::BTreeSet;

use crate::{AdSpanId, ChapterArtifact};

pub const CHAPTER_PLAYBACK_POLICY_VERSION: u32 = 1;
pub const PREVIOUS_CHAPTER_RESTART_THRESHOLD_MILLISECONDS: u64 = 3_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChapterNavigationDirection {
    Next,
    Previous,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum PlaybackSeekReason {
    UserRequested,
    NextChapter,
    PreviousChapter,
    PreviousChapterRestart,
    AutomaticAdSkip,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChapterSeekDecision {
    pub target_milliseconds: u64,
    pub reason: PlaybackSeekReason,
    pub ad_span_id: Option<AdSpanId>,
}

#[must_use]
pub fn decide_chapter_navigation(
    artifact: &ChapterArtifact,
    position_milliseconds: u64,
    direction: ChapterNavigationDirection,
) -> Option<ChapterSeekDecision> {
    match direction {
        ChapterNavigationDirection::Next => artifact
            .chapters
            .iter()
            .filter(|chapter| chapter.include_in_table_of_contents)
            .find(|chapter| chapter.start_milliseconds > position_milliseconds)
            .map(|chapter| ChapterSeekDecision {
                target_milliseconds: chapter.start_milliseconds,
                reason: PlaybackSeekReason::NextChapter,
                ad_span_id: None,
            }),
        ChapterNavigationDirection::Previous => {
            previous_chapter_decision(artifact, position_milliseconds)
        }
    }
}

fn previous_chapter_decision(
    artifact: &ChapterArtifact,
    position_milliseconds: u64,
) -> Option<ChapterSeekDecision> {
    let mut first = None;
    let mut previous = None;
    let mut current = None;
    for chapter in artifact
        .chapters
        .iter()
        .filter(|chapter| chapter.include_in_table_of_contents)
    {
        first.get_or_insert(chapter);
        if chapter.start_milliseconds > position_milliseconds {
            break;
        }
        previous = current;
        current = Some(chapter);
    }
    let current = current.or(first)?;
    let elapsed = position_milliseconds.saturating_sub(current.start_milliseconds);
    let (target, reason) = if position_milliseconds >= current.start_milliseconds
        && elapsed > PREVIOUS_CHAPTER_RESTART_THRESHOLD_MILLISECONDS
    {
        (current, PlaybackSeekReason::PreviousChapterRestart)
    } else {
        (
            previous.unwrap_or(current),
            PlaybackSeekReason::PreviousChapter,
        )
    };
    Some(ChapterSeekDecision {
        target_milliseconds: target.start_milliseconds,
        reason,
        ad_span_id: None,
    })
}

#[must_use]
pub fn decide_automatic_ad_skip(
    artifact: &ChapterArtifact,
    position_milliseconds: u64,
    enabled: bool,
    is_playing: bool,
    skipped_ad_span_ids: &BTreeSet<AdSpanId>,
) -> Option<ChapterSeekDecision> {
    if !enabled || !is_playing {
        return None;
    }
    artifact
        .ad_spans
        .iter()
        .find(|span| {
            position_milliseconds >= span.start_milliseconds
                && position_milliseconds < span.end_milliseconds
                && !skipped_ad_span_ids.contains(&span.ad_span_id)
        })
        .map(|span| ChapterSeekDecision {
            target_milliseconds: span.end_milliseconds,
            reason: PlaybackSeekReason::AutomaticAdSkip,
            ad_span_id: Some(span.ad_span_id),
        })
}
