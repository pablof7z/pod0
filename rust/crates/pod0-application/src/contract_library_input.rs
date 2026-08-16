use pod0_domain::{PodcastId, UnixTimestampMilliseconds};

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct SyntheticPodcastInput {
    /// `None` creates a new stable ID derived from the command identity.
    /// Updates and named built-ins provide their existing stable ID.
    pub podcast_id: Option<PodcastId>,
    pub title: String,
    pub author: String,
    pub image_url: Option<String>,
    pub description: String,
    pub language: Option<String>,
    pub categories: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ExternalEpisodeInput {
    pub podcast_id: PodcastId,
    pub feed_url: Option<String>,
    pub podcast_title: String,
    pub audio_url: String,
    /// Publisher's stable ID when known; audio URL is the fallback identity.
    pub guid: Option<String>,
    pub title: String,
    pub description: String,
    pub published_at: UnixTimestampMilliseconds,
    pub enclosure_mime_type: Option<String>,
    pub image_url: Option<String>,
    pub duration_milliseconds: Option<u64>,
}
