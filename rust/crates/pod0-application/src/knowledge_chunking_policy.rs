use std::collections::BTreeMap;

use pod0_domain::{SpeakerId, TranscriptSegmentRecord};

use crate::approximate_evidence_token_count;

pub(crate) fn dominant_speaker(segments: &[TranscriptSegmentRecord]) -> Option<SpeakerId> {
    let mut totals = BTreeMap::<SpeakerId, usize>::new();
    for segment in segments {
        if let Some(speaker_id) = segment.speaker_id {
            *totals.entry(speaker_id).or_default() +=
                approximate_evidence_token_count(&segment.text);
        }
    }
    totals
        .into_iter()
        .fold(None, |best, (speaker_id, tokens)| match best {
            None => Some((speaker_id, tokens)),
            Some((best_id, best_tokens))
                if tokens > best_tokens || (tokens == best_tokens && speaker_id < best_id) =>
            {
                Some((speaker_id, tokens))
            }
            current => current,
        })
        .map(|(speaker_id, _)| speaker_id)
}

pub(crate) fn snap_to_speaker_boundary(
    segments: &[TranscriptSegmentRecord],
    tokens: &[usize],
    start: usize,
    preferred_end: usize,
    window: usize,
) -> usize {
    if window == 0 || preferred_end <= start || preferred_end >= segments.len() {
        return preferred_end;
    }
    let mut best = preferred_end;
    let mut best_delta = usize::MAX;
    let mut back = preferred_end;
    let mut back_tokens = 0_usize;
    while back > start + 1 && back_tokens <= window {
        if segments[back - 1].speaker_id != segments[back].speaker_id && back_tokens < best_delta {
            best = back;
            best_delta = back_tokens;
        }
        back -= 1;
        back_tokens = back_tokens.saturating_add(tokens[back]);
    }
    let mut forward = preferred_end;
    let mut forward_tokens = 0_usize;
    while forward < segments.len() - 1 && forward_tokens <= window {
        if segments[forward].speaker_id != segments[forward + 1].speaker_id
            && forward_tokens < best_delta
        {
            best = forward + 1;
            best_delta = forward_tokens;
        }
        forward_tokens = forward_tokens.saturating_add(tokens[forward]);
        forward += 1;
    }
    best
}

pub(crate) fn compute_advance(tokens: &[usize], start: usize, end: usize, overlap: usize) -> usize {
    if overlap == 0 || end <= start {
        return end.saturating_sub(start);
    }
    let mut count = 0_usize;
    let mut running = 0_usize;
    let mut index = end;
    while index > start && running < overlap {
        index -= 1;
        running = running.saturating_add(tokens[index]);
        count += 1;
    }
    end.saturating_sub(start).saturating_sub(count).max(1)
}
