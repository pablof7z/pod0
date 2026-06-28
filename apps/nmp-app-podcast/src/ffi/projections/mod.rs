//! Snapshot projection types — narrow, Codable-friendly mirrors of the
//! Rust-side state machines surfaced via [`super::snapshot::PodcastUpdate`].
//!
//! Lives in its own module to keep [`super::snapshot`] focused on the
//! C-ABI entry points and the typed root struct. Each projection here
//! is the *external* (FFI-wire) shape; the *internal* state machines
//! it derives from live in their domain crates (`podcast-agent-core`, …)
//! or in this crate's domain modules
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
//!
//! ## Finite-float wire contract
//!
//! JSON `null` for a required (non-`Option`) numeric field causes the Swift
//! bridge decoder (`keyDecodingStrategy = .convertFromSnakeCase`) to throw
//! `keyNotFound` and drop the **entire** `PodcastUpdate` frame — the same
//! failure class as the #371 widget-CodingKeys regression.  `NaN` and
//! `Inf` serialise as JSON `null` under `serde_json` by default.
//!
//! Every required `f64`/`f32` field in this module that could ever receive a
//! NaN/Inf value uses `#[serde(serialize_with = "finite_f64_or_zero")]` (or
//! the `f32` variant) to sanitise at the wire boundary.  Pure constants (e.g.
//! `Settings::skip_forward_secs` which only ever holds user-supplied finite
//! values) are left unguarded to avoid performance overhead, but any field fed
//! from RSS-derived or LLM-derived floats must carry the attribute.

mod agent;
mod agent_context;
mod clips;
mod download;
mod identity;
mod inbox;
mod knowledge;
mod library;
mod notes;
mod platform;
mod settings;
mod social;
mod voice;

pub use agent::{
    AgentMessageSummary, AgentPickSummary, AgentSnapshot, AgentTaskSummary, ConversationsSnapshot,
    PendingApprovalSnapshot,
};
pub use agent_context::{AgentContextEpisode, AgentContextSnapshot};
pub use clips::ClipSummary;
pub use download::{DownloadItemSnapshot, DownloadQueueSnapshot};
pub use identity::{AccountSummary, OwnedPodcastInfo};
pub use inbox::InboxItem;
pub use knowledge::{KnowledgeSearchResult, MemoryFact};
pub use library::{
    CategoryBrowseItem, ChapterSummary, EpisodeSummary, NostrShowSummary, PodcastSummary,
    TranscriptEntry,
};
pub use notes::NoteSummary;
pub use platform::WidgetSnapshot;
pub use settings::SettingsSnapshot;
pub use social::{
    CommentSummary, ContactSummary, NostrConversationDTO,
    NostrConversationTurnDTO, SocialSnapshot,
};
pub use voice::VoiceState;

// ── Finite-float serde helpers ────────────────────────────────────────────────

/// Serialise a required `f64` field: replace NaN/Inf with `0.0` so the wire
/// never emits JSON `null`, which would cause the Swift bridge decoder to throw
/// `keyNotFound` and drop the entire `PodcastUpdate` frame.
///
/// Use on any required (non-`Option`) `f64` field that may receive RSS-derived
/// or LLM-derived values.  The `f32` variant is `finite_f32_or_zero`.
pub(super) fn finite_f64_or_zero<S: serde::Serializer>(v: &f64, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_f64(if v.is_finite() { *v } else { 0.0 })
}

/// Serialise a required `f32` field: replace NaN/Inf with `0.0`.
/// See [`finite_f64_or_zero`] for rationale.
pub(super) fn finite_f32_or_zero<S: serde::Serializer>(v: &f32, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_f32(if v.is_finite() { *v } else { 0.0 })
}
