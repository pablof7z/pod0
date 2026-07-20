use pod0_domain::{EpisodeId, EvidenceGenerationId, EvidenceSpanId, PodcastId};

pub(crate) fn span_key(value: EvidenceSpanId) -> String {
    stable_key(value.high, value.low)
}

pub(crate) fn generation_key(value: EvidenceGenerationId) -> String {
    stable_key(value.high, value.low)
}

pub(crate) fn episode_key(value: EpisodeId) -> String {
    stable_key(value.high, value.low)
}

pub(crate) fn podcast_key(value: PodcastId) -> String {
    stable_key(value.high, value.low)
}

pub(crate) fn parse_span_key(value: &str) -> Option<EvidenceSpanId> {
    parse_key(value).map(|(high, low)| EvidenceSpanId::from_parts(high, low))
}

pub(crate) fn parse_generation_key(value: &str) -> Option<EvidenceGenerationId> {
    parse_key(value).map(|(high, low)| EvidenceGenerationId::from_parts(high, low))
}

pub(crate) fn parse_episode_key(value: &str) -> Option<EpisodeId> {
    parse_key(value).map(|(high, low)| EpisodeId::from_parts(high, low))
}

fn stable_key(high: u64, low: u64) -> String {
    format!("{high:016x}{low:016x}")
}

fn parse_key(value: &str) -> Option<(u64, u64)> {
    (value.len() == 32).then_some(())?;
    Some((
        u64::from_str_radix(&value[..16], 16).ok()?,
        u64::from_str_radix(&value[16..], 16).ok()?,
    ))
}
