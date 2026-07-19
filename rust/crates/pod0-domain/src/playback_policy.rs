use crate::{EpisodeRecord, PlaybackSegment, UnixTimestampMilliseconds};

pub const RESUME_END_GUARD_MILLISECONDS: u64 = 5_000;
pub const POSITION_COMMIT_INTERVAL_MILLISECONDS: i64 = 30_000;
pub const MEANINGFUL_LISTENING_THRESHOLD_MILLISECONDS: u64 = 300_000;

#[must_use]
pub const fn meaningful_listening_reached(position_milliseconds: u64) -> bool {
    position_milliseconds >= MEANINGFUL_LISTENING_THRESHOLD_MILLISECONDS
}

#[must_use]
pub fn playback_start_position(episode: &EpisodeRecord, segment: Option<PlaybackSegment>) -> u64 {
    if let Some(start) = segment.and_then(|value| value.start_position_milliseconds) {
        return bounded_position(start, episode.duration_milliseconds);
    }
    let resume = episode.listening.resume_position_milliseconds;
    let Some(duration) = episode.duration_milliseconds else {
        return resume;
    };
    if resume > 0 && resume.saturating_add(RESUME_END_GUARD_MILLISECONDS) < duration {
        resume
    } else {
        0
    }
}

#[must_use]
pub fn segment_reached(position_milliseconds: u64, segment: Option<PlaybackSegment>) -> bool {
    segment
        .and_then(|value| value.end_position_milliseconds)
        .is_some_and(|end| position_milliseconds >= end)
}

#[must_use]
pub fn should_commit_position(
    durable_position_milliseconds: u64,
    observed_position_milliseconds: u64,
    last_committed_at: Option<UnixTimestampMilliseconds>,
    observed_at: UnixTimestampMilliseconds,
    force: bool,
) -> bool {
    if force || durable_position_milliseconds == 0 {
        return observed_position_milliseconds != durable_position_milliseconds;
    }
    if observed_position_milliseconds == durable_position_milliseconds {
        return false;
    }
    last_committed_at.is_none_or(|last| {
        observed_at.value.saturating_sub(last.value) >= POSITION_COMMIT_INTERVAL_MILLISECONDS
    })
}

#[must_use]
pub fn bounded_position(position_milliseconds: u64, duration_milliseconds: Option<u64>) -> u64 {
    duration_milliseconds.map_or(position_milliseconds, |duration| {
        position_milliseconds.min(duration)
    })
}
