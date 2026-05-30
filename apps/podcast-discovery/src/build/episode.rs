//! Build the tag set for a `kind:54` episode event (NIP-F4) from an [`Episode`].
//!
//! NIP-F4 episodes have no `d`, `a`, or `published_at` tags — the episode is a
//! regular (non-replaceable) event identified by its event `id`, and the parent
//! show is implicit from the shared podcast pubkey. Audio is represented as
//! `["audio", "<url>", "<mime>"]` instead of the old `imeta` block.

use podcast_core::types::episode::Episode;

/// Overrides for the `audio` tag. When a field is absent the builder falls
/// back to the episode's own value.
///
/// * `url` — M8 Blossom upload. The `kind:54` `audio` tag normally carries
///   `episode.enclosure_url` (the RSS enclosure). After a successful Blossom
///   upload the publish path supplies the permanent Blossom URL here so the
///   published event points at the hosted blob instead of the original feed.
/// * `mime_type` — overrides `episode.enclosure_mime_type` (then `"audio/mp4"`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImetaInfo {
    pub url: Option<String>,
    pub mime_type: Option<String>,
}

/// Build the canonical tags for a `kind:54` event (NIP-F4). Convenience wrapper
/// around [`episode_to_episode_tags_with_imeta`] for callers without a mime
/// override.
pub fn episode_to_episode_tags(episode: &Episode) -> Vec<Vec<String>> {
    episode_to_episode_tags_with_imeta(episode, &ImetaInfo::default())
}

/// Build the canonical tags for a `kind:54` event (NIP-F4) with an optional
/// mime-type override on the `audio` tag.
pub fn episode_to_episode_tags_with_imeta(
    episode: &Episode,
    imeta: &ImetaInfo,
) -> Vec<Vec<String>> {
    let mut tags: Vec<Vec<String>> = vec![vec!["title".into(), episode.title.clone()]];
    if !episode.description.is_empty() {
        tags.push(vec!["description".into(), episode.description.clone()]);
    }
    if let Some(dur) = episode.duration_secs {
        tags.push(vec!["duration".into(), (dur as i64).to_string()]);
    }
    if let Some(image) = &episode.image_url {
        tags.push(vec!["image".into(), image.as_str().to_string()]);
    }
    tags.push(build_audio_tag(episode, imeta));
    if let Some(chapters) = &episode.chapters_url {
        tags.push(vec![
            "chapters".into(),
            chapters.as_str().to_string(),
            "application/json+chapters".into(),
        ]);
    }
    if let Some(transcript) = &episode.publisher_transcript_url {
        tags.push(vec![
            "transcript".into(),
            transcript.as_str().to_string(),
            transcript_mime(&episode.publisher_transcript_type),
        ]);
    }
    tags
}

fn build_audio_tag(episode: &Episode, imeta: &ImetaInfo) -> Vec<String> {
    let mime = imeta
        .mime_type
        .clone()
        .or_else(|| episode.enclosure_mime_type.clone())
        .unwrap_or_else(|| "audio/mp4".into());
    let url = imeta
        .url
        .clone()
        .unwrap_or_else(|| episode.enclosure_url.to_string());
    vec!["audio".into(), url, mime]
}

fn transcript_mime(kind: &Option<podcast_core::types::transcript::TranscriptKind>) -> String {
    use podcast_core::types::transcript::TranscriptKind;
    match kind {
        Some(TranscriptKind::Vtt) => "text/vtt".into(),
        Some(TranscriptKind::Srt) => "application/x-subrip".into(),
        Some(TranscriptKind::Json) => "application/json".into(),
        Some(TranscriptKind::Html) => "text/html".into(),
        Some(TranscriptKind::Text) => "text/plain".into(),
        None => "text/vtt".into(),
    }
}

#[cfg(test)]
#[path = "episode_tests.rs"]
mod tests;
