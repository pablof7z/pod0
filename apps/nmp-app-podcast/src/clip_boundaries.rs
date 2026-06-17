use uuid::Uuid;

const AUTOSNIP_LOOKBACK_SECS: f64 = 90.0;
const AUTOSNIP_LEAD_SECS: f64 = 15.0;
const AUTOSNIP_FALLBACK_LOOKBACK_SECS: f64 = 30.0;
const AUTOSNIP_FALLBACK_LEAD_SECS: f64 = 30.0;
const AUTOSNIP_TARGET_MIN_SECS: f64 = 30.0;
const AUTOSNIP_TARGET_MAX_SECS: f64 = 60.0;
const AUTOSNIP_AFTER_TAP_GRACE_SECS: f64 = 5.0;
const QUOTE_TARGET_MIN_SECS: f64 = 10.0;
const QUOTE_TARGET_MAX_SECS: f64 = 25.0;
const QUOTE_AFTER_TAP_GRACE_SECS: f64 = 2.0;

#[derive(Debug, PartialEq)]
pub(crate) struct AutoSnipBounds {
    pub(crate) start_secs: f64,
    pub(crate) end_secs: f64,
    pub(crate) transcript_text: String,
    pub(crate) speaker: Option<String>,
    pub(crate) status: String,
}

pub(crate) struct ClipText {
    pub(crate) transcript_text: String,
    pub(crate) speaker: Option<String>,
}

pub(crate) fn fallback_auto_snip_bounds(pos: f64, duration: Option<f64>) -> (f64, f64) {
    let raw_start = pos - AUTOSNIP_FALLBACK_LOOKBACK_SECS;
    let raw_end = pos + AUTOSNIP_FALLBACK_LEAD_SECS;
    let start = raw_start.max(0.0);
    let end = match duration {
        Some(d) if d > 0.0 => raw_end.min(d),
        _ => raw_end,
    };
    (start, end)
}

pub(crate) fn resolve_auto_snip_bounds(
    entries: &[podcast_transcripts::TranscriptEntry],
    playhead_secs: f64,
    duration: Option<f64>,
) -> Option<AutoSnipBounds> {
    resolve_transcript_bounds(
        entries,
        playhead_secs,
        duration,
        AUTOSNIP_TARGET_MIN_SECS,
        AUTOSNIP_TARGET_MAX_SECS,
        AUTOSNIP_AFTER_TAP_GRACE_SECS,
        "transcript_refined",
    )
}

pub(crate) fn resolve_quote_bounds(
    entries: &[podcast_transcripts::TranscriptEntry],
    playhead_secs: f64,
    duration: Option<f64>,
) -> Option<AutoSnipBounds> {
    resolve_transcript_bounds(
        entries,
        playhead_secs,
        duration,
        QUOTE_TARGET_MIN_SECS,
        QUOTE_TARGET_MAX_SECS,
        QUOTE_AFTER_TAP_GRACE_SECS,
        "quote_resolved",
    )
}

pub(crate) fn resolve_manual_clip_bounds(
    entries: &[podcast_transcripts::TranscriptEntry],
    start_secs: f64,
    end_secs: f64,
    duration: Option<f64>,
) -> Option<AutoSnipBounds> {
    let mut selected: Vec<&podcast_transcripts::TranscriptEntry> = entries
        .iter()
        .filter(|entry| {
            entry.end_secs > entry.start_secs
                && entry.end_secs > start_secs
                && entry.start_secs < end_secs
                && !entry.text.trim().is_empty()
        })
        .collect();
    if selected.is_empty() {
        return None;
    }
    selected.sort_by(|a, b| a.start_secs.total_cmp(&b.start_secs));
    let start_secs = selected.first()?.start_secs.max(0.0);
    let end_secs = clamp_to_duration(selected.last()?.end_secs, duration);
    if end_secs <= start_secs {
        return None;
    }
    Some(AutoSnipBounds {
        start_secs,
        end_secs,
        transcript_text: selected
            .iter()
            .map(|entry| entry.text.trim())
            .collect::<Vec<_>>()
            .join(" "),
        speaker: dominant_speaker(&selected),
        status: "manual".to_owned(),
    })
}

fn resolve_transcript_bounds(
    entries: &[podcast_transcripts::TranscriptEntry],
    playhead_secs: f64,
    duration: Option<f64>,
    target_min_secs: f64,
    target_max_secs: f64,
    after_tap_grace_secs: f64,
    status: &str,
) -> Option<AutoSnipBounds> {
    let lo = (playhead_secs - AUTOSNIP_LOOKBACK_SECS).max(0.0);
    let hi = clamp_to_duration(playhead_secs + AUTOSNIP_LEAD_SECS, duration);
    let mut window: Vec<&podcast_transcripts::TranscriptEntry> = entries
        .iter()
        .filter(|entry| {
            let end = entry.end_secs;
            end > entry.start_secs
                && end >= lo
                && entry.start_secs <= hi
                && !entry.text.trim().is_empty()
        })
        .collect();
    window.sort_by(|a, b| a.start_secs.total_cmp(&b.start_secs));
    if window.is_empty() {
        return None;
    }
    let anchor = window
        .iter()
        .rposition(|entry| entry.start_secs <= playhead_secs)
        .unwrap_or(0);
    let mut start = anchor;
    let mut end = anchor;
    while end + 1 < window.len()
        && window[end].end_secs < playhead_secs + after_tap_grace_secs
    {
        let next_end = window[end + 1].end_secs;
        if next_end - window[start].start_secs > target_max_secs {
            break;
        }
        end += 1;
    }
    while start > 0 {
        let current_end = window[end].end_secs;
        let candidate_start = window[start - 1].start_secs;
        if current_end - candidate_start > target_max_secs {
            break;
        }
        start -= 1;
        if current_end - window[start].start_secs >= target_min_secs {
            break;
        }
    }
    let start_secs = window[start].start_secs.max(0.0);
    let end_secs = clamp_to_duration(window[end].end_secs, duration);
    if end_secs <= start_secs {
        return None;
    }
    let selected = &window[start..=end];
    Some(AutoSnipBounds {
        start_secs,
        end_secs,
        transcript_text: selected
            .iter()
            .map(|entry| entry.text.trim())
            .collect::<Vec<_>>()
            .join(" "),
        speaker: dominant_speaker(selected),
        status: status.to_owned(),
    })
}

pub(crate) fn usable_clip_id(candidate: Option<String>) -> String {
    candidate
        .filter(|id| Uuid::parse_str(id).is_ok())
        .unwrap_or_else(|| Uuid::new_v4().to_string())
}

fn clamp_to_duration(value: f64, duration: Option<f64>) -> f64 {
    match duration {
        Some(d) if d > 0.0 => value.min(d),
        _ => value,
    }
}

fn dominant_speaker(entries: &[&podcast_transcripts::TranscriptEntry]) -> Option<String> {
    let first = entries.first()?.speaker.as_ref()?;
    if entries.iter().all(|entry| entry.speaker.as_ref() == Some(first)) {
        Some(first.clone())
    } else {
        None
    }
}
