use pod0_domain::{CancellationId, CommandId, HostRequestId, StateRevision};

pub const MAX_LIBRARY_DOCUMENT_BYTES: u64 = 10 * 1_024 * 1_024;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LibraryNetworkIntent {
    DirectorySearch {
        query: String,
        limit: u16,
    },
    TopPodcasts {
        storefront: String,
        limit: u16,
    },
    SharedEpisodeImport {
        source_url: String,
    },
    CatalogEpisodeSearch {
        episode_query: String,
        podcast_hint: Option<String>,
        limit: u16,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Record)]
pub struct LibraryHttpRequest {
    pub url: String,
    pub accept: String,
    pub maximum_response_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Enum)]
pub enum LibraryNetworkStep {
    DirectorySearch,
    TopChart,
    DirectoryLookup {
        ranked_ids: Vec<u64>,
    },
    SharedPage,
    SharedAppleLookup {
        page: EpisodeWebPageMetadata,
    },
    SharedFeed {
        page: EpisodeWebPageMetadata,
    },
    CatalogDirectory,
    CatalogFeed {
        feed_urls: Vec<String>,
        ordinal: u16,
        candidates: Vec<CatalogEpisodeCandidate>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DurableLibraryNetworkEffectRequest {
    pub request_id: HostRequestId,
    pub command_id: CommandId,
    pub cancellation_id: CancellationId,
    pub issued_revision: StateRevision,
    pub deadline_at: Option<pod0_domain::UnixTimestampMilliseconds>,
    pub step: LibraryNetworkStep,
    pub http: LibraryHttpRequest,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Record)]
pub struct PodcastDirectoryEntry {
    pub collection_id: u64,
    pub collection_name: String,
    pub artist_name: Option<String>,
    pub feed_url: String,
    pub artwork_url: Option<String>,
    pub primary_genre_name: Option<String>,
    pub track_count: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Record)]
pub struct EpisodeWebPageMetadata {
    pub episode_title: Option<String>,
    pub podcast_title: Option<String>,
    pub description: Option<String>,
    pub published_at_milliseconds: Option<i64>,
    pub duration_milliseconds: Option<u64>,
    pub audio_url: Option<String>,
    pub audio_mime_type: Option<String>,
    pub image_url: Option<String>,
    pub feed_url: Option<String>,
    pub canonical_url: Option<String>,
    pub apple_podcast_id: Option<String>,
    pub guid: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Record)]
pub struct LibraryDocumentObservation {
    pub bytes: Vec<u8>,
    pub response_url: String,
    pub mime_type: Option<String>,
    pub http_status: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Record)]
pub struct ResolvedSharedEpisode {
    pub podcast_id: pod0_domain::PodcastId,
    pub podcast_title: String,
    pub feed_url: Option<String>,
    pub audio_url: String,
    pub guid: Option<String>,
    pub title: String,
    pub description: String,
    pub published_at_milliseconds: i64,
    pub enclosure_mime_type: Option<String>,
    pub image_url: Option<String>,
    pub duration_milliseconds: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Record)]
pub struct CatalogEpisodeCandidate {
    pub episode: ResolvedSharedEpisode,
    pub score: u32,
}
