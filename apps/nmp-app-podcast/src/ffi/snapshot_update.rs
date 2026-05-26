//! [`PodcastUpdate`] — the typed root of the snapshot JSON.
//!
//! Kept in its own file so `snapshot.rs` can stay focused on the builder
//! logic and FFI entry points without approaching the 500-line hard limit.

use serde::{Deserialize, Serialize};

use super::projections::{
    AccountSummary, AgentPickSummary, AgentSnapshot, AgentTaskSummary, BriefingSnapshot,
    CategoryBrowseItem, ClipSummary, CommentSummary, DownloadQueueSnapshot, EpisodeSummary,
    InboxItem, KnowledgeSearchResult, MemoryFact, NostrShowSummary, OwnedPodcastInfo,
    PodcastSummary, SettingsSnapshot, SocialSnapshot, TtsEpisodeSummary, VoiceState,
    WidgetSnapshot, WikiArticle,
};
use crate::player::PlayerState;

/// Typed root of the snapshot JSON.
///
/// `running`, `rev`, and `schema_version` mirror the kernel's existing
/// tick contract. Forward compatibility is via Swift's `Codable` tolerating
/// unknown fields; backward compatibility is gated by `schema_version` —
/// bump it only when removing or renaming a field.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PodcastUpdate {
    /// `true` once the kernel is running. False during shutdown.
    pub running: bool,
    /// Monotonically increasing revision id; iOS uses it to dedupe ticks.
    pub rev: u64,
    /// Schema version — bump on incompatible shape changes.
    pub schema_version: u32,
    /// Active player projection, or `None` when nothing is loaded.
    ///
    /// Per D5 the field is `null` when no episode is loaded so the
    /// iOS decoder doesn't render a hero with default zeros.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub now_playing: Option<PlayerState>,
    /// Active download-queue projection, or `None` when no downloads
    /// have ever been enqueued during this kernel lifetime.
    ///
    /// Per D5 we serialize `None` (not an empty struct) when there is
    /// nothing to show — keeps the byte-compatible legacy stub for
    /// "no-op snapshot" intact.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub downloads: Option<DownloadQueueSnapshot>,
    /// Agent-chat projection: the ordered message transcript of the
    /// active conversation plus an `is_busy` flag.
    ///
    /// `None` until the first agent turn lands during a kernel lifetime —
    /// preserves byte-identity with the legacy stub.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentSnapshot>,
    /// Voice projection: whether TTS is currently speaking and (when
    /// it is) the in-flight request id + active voice id.
    ///
    /// `None` while no voice session is active — preserves byte-
    /// identity with the legacy stub for non-voice-mode snapshots.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice: Option<VoiceState>,
    /// Briefing projection: lifecycle status of the current briefing
    /// (if any) + segment count + minutes until the next scheduled
    /// slot. `None` when the scheduler has never been touched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub briefing: Option<BriefingSnapshot>,
    /// Social projection: the active account's NIP-02 (kind:3) follow
    /// list. `None` until the NMP substrate contact store is wired into
    /// the projection layer — tracked in BACKLOG (`pr-social-graph-nmp-store-wiring`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub social: Option<SocialSnapshot>,
    /// Subscribed-podcast library projection. Each entry is a narrow
    /// [`PodcastSummary`] with embedded episode rows (newest-first).
    /// Empty until the first successful `podcast.subscribe` action.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub library: Vec<PodcastSummary>,
    /// Active Nostr identity, or `None` when no account is loaded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_account: Option<AccountSummary>,
    /// Platform-integration projection: the narrow slice the iOS widget
    /// extension, Live Activity, Handoff, and Siri executors need.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub widget: Option<WidgetSnapshot>,
    /// Transient toast message the kernel wants the host to surface.
    /// `None` on every tick without a fresh message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toast: Option<String>,
    /// iTunes search results, populated after a `podcast.search_itunes` action.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub search_results: Vec<PodcastSummary>,
    /// NIP-F4 discovery results, populated after `podcast.discover_nostr`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nostr_results: Vec<NostrShowSummary>,
    /// App-settings projection (onboarding completion, auto-skip-ads, …).
    ///
    /// Defaults to the fresh-install `SettingsSnapshot`. The
    /// `skip_serializing_if = "SettingsSnapshot::is_default"` guard keeps the
    /// no-op snapshot byte-identical to the legacy stub (D6).
    #[serde(default, skip_serializing_if = "SettingsSnapshot::is_default")]
    pub settings: SettingsSnapshot,
    /// NIP-22 (kind 1111) comments for the currently-playing episode.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub comments: Vec<CommentSummary>,
    /// Playback "Up Next" queue, front-first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub queue: Vec<EpisodeSummary>,
    /// AI-wiki articles surfaced to the iOS reader.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wiki_articles: Vec<WikiArticle>,
    /// Filtered result of the most recent `podcast.wiki.search` dispatch.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wiki_search_results: Vec<WikiArticle>,
    /// AI agent picks for the Home rail.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub picks: Vec<AgentPickSummary>,
    /// Agent-scheduled-tasks projection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agent_tasks: Vec<AgentTaskSummary>,
    /// RAG / knowledge search results.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub knowledge_search_results: Vec<KnowledgeSearchResult>,
    /// Agent-memory bag (feature #33).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub memory_facts: Vec<MemoryFact>,
    /// Agent-generated TTS episode list (feature #43).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tts_episodes: Vec<TtsEpisodeSummary>,
    /// User-saved audio clips across all episodes, newest-first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub clips: Vec<ClipSummary>,
    /// AI-triaged inbox: unlistened episodes, highest-priority-first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inbox: Vec<InboxItem>,
    /// User-owned podcasts (NIP-F4).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub owned_podcasts: Vec<OwnedPodcastInfo>,
    /// Browse-by-topic aggregation surfaced via the iOS Library tab.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<CategoryBrowseItem>,
}

impl Default for PodcastUpdate {
    fn default() -> Self {
        Self {
            running: true,
            rev: 0,
            schema_version: 1,
            now_playing: None,
            downloads: None,
            agent: None,
            voice: None,
            briefing: None,
            social: None,
            library: Vec::new(),
            active_account: None,
            widget: None,
            toast: None,
            search_results: Vec::new(),
            nostr_results: Vec::new(),
            settings: SettingsSnapshot::default(),
            comments: Vec::new(),
            queue: Vec::new(),
            wiki_articles: Vec::new(),
            wiki_search_results: Vec::new(),
            picks: Vec::new(),
            agent_tasks: Vec::new(),
            knowledge_search_results: Vec::new(),
            memory_facts: Vec::new(),
            tts_episodes: Vec::new(),
            clips: Vec::new(),
            inbox: Vec::new(),
            owned_podcasts: Vec::new(),
            categories: Vec::new(),
        }
    }
}
