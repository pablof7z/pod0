//! `podcast-core` — pure domain layer for the podcast app.
//!
//! Domain types ported from the legacy Swift layer. No FFI, no kernel deps;
//! `nmp-core` integration arrives in later milestones when projections need
//! a kernel observer.

pub mod migration;
pub mod projections;
pub mod types;

pub use projections::{EpisodeProjection, EpisodeSummary, LibraryProjection, PodcastSummary};
pub use types::{
    AdKind, AdSegment, Anchor, AutoDownloadMode, AutoDownloadPolicy, CategorySettings, Chapter,
    Clip, ClipBoundary, ClipSource, DownloadState, ElevenLabsCredentialSource, Episode, EpisodeId,
    GenerationSource, HeadphoneGestureAction, NostrVisibility, OllamaCredentialSource,
    OpenRouterCredentialSource, Person, Podcast, PodcastCategory, PodcastId, PodcastKind,
    PodcastSubscription, ProviderCredentialMetadata, Settings, SoundBite, SttProvider,
    TranscriptKind, TranscriptSource, TranscriptState, TriageDecision,
};
