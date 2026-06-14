//! [`PodcastUpdate`] — the typed root of the snapshot JSON.
//!
//! Kept in its own file so `snapshot.rs` can stay focused on the builder
//! logic and FFI entry points without approaching the 500-line hard limit.

use serde::{Deserialize, Serialize};

use nmp_feedback::FeedbackThreadDto;

use super::projections::{
    AccountSummary, AgentContextSnapshot, AgentPickSummary, AgentSnapshot,
    AgentTaskSummary, CategoryBrowseItem, ClipSummary, CommentSummary, DownloadQueueSnapshot,
    EpisodeSummary, InboxItem, KnowledgeSearchResult, MemoryFact, NostrConversationDTO,
    NostrShowSummary, OwnedPodcastInfo, PodcastSummary, SettingsSnapshot, SocialSnapshot,
    VoiceState, WidgetSnapshot, WikiArticle,
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
    /// Agent-prompt inventory context: kernel-owned selection/ordering/capping
    /// of the subscribed-show list, in-progress episodes, and recent-unplayed
    /// episodes the iOS `AgentPrompt` builder renders into its system prompt.
    ///
    /// `None` when the library is empty (nothing to surface) — preserves
    /// byte-identity with the legacy stub for fresh installs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_context: Option<AgentContextSnapshot>,
    /// Voice projection: whether TTS is currently speaking and (when
    /// it is) the in-flight request id + active voice id.
    ///
    /// `None` while no voice session is active — preserves byte-
    /// identity with the legacy stub for non-voice-mode snapshots.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice: Option<VoiceState>,
    /// Social projection: the active account's NIP-02 (kind:3) follow
    /// list. `None` until the NMP substrate contact store is wired into
    /// the projection layer — tracked in BACKLOG (`pr-social-graph-nmp-store-wiring`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub social: Option<SocialSnapshot>,
    /// Known-podcast library projection. Each entry is a narrow
    /// [`PodcastSummary`] with embedded episode rows (newest-first) and an
    /// explicit `is_subscribed` follow flag.
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
    /// Defaults to the fresh-install `SettingsSnapshot` and is **always**
    /// serialized — the kernel no longer omits it when all-default. This keeps
    /// the wire shape uniform so a decoding shell never has to distinguish
    /// "absent" from "default", and the cross-language fixture test can assert
    /// a stable byte image.
    #[serde(default)]
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
    /// User-saved audio clips across all episodes, newest-first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub clips: Vec<ClipSummary>,
    /// AI-triaged inbox: unlistened episodes, highest-priority-first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inbox: Vec<InboxItem>,
    /// `true` while a background LLM triage pass is running. The iOS UI
    /// can show a spinner on the Inbox tab while this is set.
    /// Omitted from the wire when `false` (D5).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub inbox_triage_in_progress: bool,
    /// Unix seconds for the most recent successful inbox triage pass.
    ///
    /// `None` until the first Ready triage cache entry exists. Pending retry
    /// placeholders do not count as completed triage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inbox_last_triaged_at: Option<i64>,
    /// User-owned podcasts (NIP-F4).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub owned_podcasts: Vec<OwnedPodcastInfo>,
    /// Browse-by-topic aggregation surfaced via the iOS Library tab.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<CategoryBrowseItem>,
    /// NIP-10-threaded Nostr conversations between the active account and
    /// its peers, newest-first by last_activity. Each conversation merges
    /// inbound kind:1 notes + outbound auto-responder turns under a common
    /// root event id. Empty until the first `FetchAgentNotes` dispatch or
    /// outbound auto-reply. Subsumes the LEGACY flat `agent_notes` list for
    /// UI purposes — shells should prefer this field for conversation views.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nostr_conversations: Vec<NostrConversationDTO>,
    /// User-configured app relays (NMP v0.2.1 `configured_relays` projection),
    /// each carrying the NIP-65 role string. Projected from the kernel's
    /// `AppRelaySlot` (`NmpApp::configured_relays_handle`) — NOT from
    /// `PodcastStore`, since relay state is kernel-owned. Empty until the
    /// actor seeds `initial_relays` at `Start` or the user adds a relay via
    /// `podcast.settings.add_relay`. Drives the iOS App Relays editor.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub configured_relays: Vec<AppRelayRow>,
    /// In-app feedback events (TENEX project notes), as `SignedNostrEvent`-shaped
    /// JSON objects (`{id,pubkey,created_at,kind,tags,content,sig}` — `pubkey` is
    /// the event author, `sig` is `""`). kind:1 messages/replies + kind:513
    /// metadata, all bearing the project `["a"]` coord. Empty until the first
    /// `FetchFeedback` dispatch. The iOS `FeedbackStore` rebuilds threads from
    /// this flat list (replacing the deleted `FeedbackRelayClient` WebSocket
    /// fetch). Raw `serde_json::Value` rows because the host caches inbound
    /// kernel events as JSON, not typed `nostr::Event`s.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub feedback_events: Vec<serde_json::Value>,
    /// Resolved feedback threads (#354): kind:1 roots (newest-first) with
    /// their replies (oldest-first) and the newest-wins kind:513 metadata,
    /// reduced kernel-side from `feedback_events`. The shell renders this
    /// directly instead of re-running the Nostr reduction.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub feedback_threads: Vec<FeedbackThreadDto>,
}

/// One row of the `configured_relays` projection: a relay URL plus its
/// NIP-65 role string (`read` | `write` | `both` | `indexer`, optionally
/// comma-joined e.g. `both,indexer`). Mirrors `nmp_core::kernel::AppRelay`'s
/// `url()` / `role()` accessors. Kept narrow so the iOS shell renders a role
/// badge / picker without parsing transport internals.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct AppRelayRow {
    pub url: String,
    pub role: String,
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
            agent_context: None,
            voice: None,
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
            clips: Vec::new(),
            inbox: Vec::new(),
            inbox_triage_in_progress: false,
            inbox_last_triaged_at: None,
            owned_podcasts: Vec::new(),
            categories: Vec::new(),
            nostr_conversations: Vec::new(),
            configured_relays: Vec::new(),
            feedback_events: Vec::new(),
            feedback_threads: Vec::new(),
        }
    }
}
