//! Snapshot projection types — narrow, Codable-friendly mirrors of the
//! Rust-side state machines surfaced via [`super::snapshot::PodcastUpdate`].
//!
//! Lives in its own module to keep [`super::snapshot`] focused on the
//! C-ABI entry points and the typed root struct. Each projection here
//! is the *external* (FFI-wire) shape; the *internal* state machines
//! it derives from live in their domain crates (`podcast-briefings`,
//! `podcast-agent-core`, …) or in this crate's domain modules
//! (`crate::player`, `crate::download`).
//!
//! ## Structure
//!
//! Split into focused sub-modules to keep each file under the 500-LOC
//! hard ceiling (AGENTS.md). All types are re-exported flat so existing
//! `crate::ffi::projections::Foo` import paths stay valid.
//!
//! ## D7 / D6
//!
//! These structs are pure data: Swift `Codable` decodes them and renders.
//! No conditional logic, no policy decisions.

mod agent;
mod briefing;
mod clips;
mod download;
mod identity;
mod inbox;
mod knowledge;
mod library;
mod platform;
mod settings;
mod social;
mod tts;
mod voice;

pub use agent::{
    AgentMessageSummary, AgentPickSummary, AgentSnapshot, AgentTaskSummary,
    ConversationsSnapshot, PendingApprovalSnapshot,
};
pub use briefing::{BriefingSegmentSummary, BriefingSnapshot};
pub use clips::ClipSummary;
pub use download::{DownloadItemSnapshot, DownloadQueueSnapshot};
pub use identity::{AccountSummary, OwnedPodcastInfo};
pub use inbox::InboxItem;
pub use knowledge::{KnowledgeSearchResult, MemoryFact, WikiArticle};
pub use library::{
    CategoryBrowseItem, ChapterSummary, EpisodeSummary, NostrShowSummary, PodcastSummary,
    TranscriptEntry,
};
pub use platform::WidgetSnapshot;
pub use settings::SettingsSnapshot;
pub use social::{CommentSummary, ContactSummary, SocialSnapshot};
pub use tts::TtsEpisodeSummary;
pub use voice::VoiceState;
