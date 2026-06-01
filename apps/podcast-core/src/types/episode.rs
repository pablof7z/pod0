use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use url::Url;
use uuid::Uuid;

use crate::types::ad_segment::AdSegment;
use crate::types::chapter::Chapter;
use crate::types::download::DownloadState;
use crate::types::generation_source::GenerationSource;
use crate::types::person::Person;
use crate::types::podcast::PodcastId;
use crate::types::soundbite::SoundBite;
use crate::types::transcript::{TranscriptKind, TranscriptState};
use crate::types::triage::TriageDecision;

/// Namespace UUID scoped to `(feed_url, guid)` derived episode ids.
///
/// Distinct from `podcast-discovery`'s NIP-74 d-tag namespace so the same
/// publisher d-tag and an RSS guid never collide. Treat as a constant —
/// changing it would re-randomize every persisted episode id on next refresh.
const EPISODE_NS: Uuid = Uuid::from_bytes([
    0xe1, 0x53, 0x90, 0x4c, 0xb2, 0x47, 0x5a, 0x4b, 0x9f, 0x0e, 0x83, 0x24, 0xb7, 0x5c, 0xd1, 0x2e,
]);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EpisodeId(pub Uuid);

impl EpisodeId {
    pub fn new(id: Uuid) -> Self {
        Self(id)
    }

    /// Random episode id. Retained for tests and other throwaway contexts
    /// where stability is not required. **Do not** call this from any code
    /// path that persists episodes — feeding `Uuid::new_v4()` into the
    /// store breaks position persistence, download path lookups, and
    /// auto-download dedup the moment the same episode is re-parsed.
    /// Use [`EpisodeId::from_feed_and_guid`] instead.
    pub fn generate() -> Self {
        Self(Uuid::new_v4())
    }

    /// Deterministic id derived from the episode's feed URL and the publisher
    /// guid (or a synthesized stand-in for feeds without `<guid>`). UUIDv5
    /// over `"{feed_url}|{guid}"` so a re-fetch of the same item always
    /// produces the same `EpisodeId`. This is the only stable key we have
    /// across refreshes; everything keyed off `EpisodeId` (positions,
    /// `local_paths`, auto-download dedup) depends on it.
    pub fn from_feed_and_guid(feed_url: &str, guid: &str) -> Self {
        let key = format!("{feed_url}|{guid}");
        Self(Uuid::new_v5(&EPISODE_NS, key.as_bytes()))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Episode {
    pub id: EpisodeId,
    pub podcast_id: PodcastId,
    pub guid: String,

    pub title: String,
    pub description: String,
    pub pub_date: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<f64>,

    pub enclosure_url: Url,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enclosure_mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<Url>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub chapters: Option<Vec<Chapter>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub persons: Option<Vec<Person>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sound_bites: Option<Vec<SoundBite>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher_transcript_url: Option<Url>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher_transcript_type: Option<TranscriptKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chapters_url: Option<Url>,

    pub position_secs: f64,
    pub played: bool,
    pub is_starred: bool,
    pub download_state: DownloadState,
    pub transcript_state: TranscriptState,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub ad_segments: Option<Vec<AdSegment>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation_source: Option<GenerationSource>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub triage_decision: Option<TriageDecision>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub triage_rationale: Option<String>,
    pub triage_is_hero: bool,

    pub metadata_indexed: bool,

    /// AI-generated 2–3 sentence episode summary, produced by the kernel's
    /// `episode_summary_llm` pass over the title + description (+ transcript
    /// when present). `None` until the user invokes the `summarize_episode`
    /// agent tool. Persisted alongside the episode so an expensive LLM call
    /// survives feed refreshes and app restarts (mirrors `triage_rationale` /
    /// `chapters`); `skip_serializing_if` keeps old `podcasts.json` records
    /// forward-compatible.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

impl Episode {
    /// Construct an episode with a deterministic [`EpisodeId`] derived from
    /// `feed_url` and `guid`. The signature takes `feed_url` explicitly so the
    /// id-stability invariant is enforced at the type level: every call site
    /// has to provide the feed identity, and there is no path that quietly
    /// falls back to a random id.
    ///
    /// For non-RSS sources where the canonical identifier is not a feed URL
    /// (e.g. NIP-74 d-tags), callers may pass an opaque namespace string in
    /// place of `feed_url` and then override `episode.id` with a
    /// source-specific derivation — the discovery crate does this.
    pub fn new(
        podcast_id: PodcastId,
        feed_url: &str,
        guid: impl Into<String>,
        title: impl Into<String>,
        enclosure_url: Url,
        pub_date: DateTime<Utc>,
    ) -> Self {
        let guid = guid.into();
        Self {
            id: EpisodeId::from_feed_and_guid(feed_url, &guid),
            podcast_id,
            guid,
            title: title.into(),
            description: String::new(),
            pub_date,
            duration_secs: None,
            enclosure_url,
            enclosure_mime_type: None,
            image_url: None,
            chapters: None,
            persons: None,
            sound_bites: None,
            publisher_transcript_url: None,
            publisher_transcript_type: None,
            chapters_url: None,
            position_secs: 0.0,
            played: false,
            is_starred: false,
            download_state: DownloadState::default(),
            transcript_state: TranscriptState::default(),
            ad_segments: None,
            generation_source: None,
            triage_decision: None,
            triage_rationale: None,
            triage_is_hero: false,
            metadata_indexed: false,
            summary: None,
        }
    }

    pub fn id(&self) -> EpisodeId {
        self.id
    }
}

#[cfg(test)]
#[path = "episode_tests.rs"]
mod tests;
