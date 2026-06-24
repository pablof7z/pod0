//! Compound podcast ActionModule — routes all `"podcast.*"` dispatches.
//!
//! Swift encodes every podcast action as `{"op":"<variant>", ...fields}`.
//! The `#[serde(tag = "op", rename_all = "snake_case")]` discriminator maps
//! the string `op` value to the enum variant. The module's `execute` body
//! forwards the whole action as `ActorCommand::DispatchHostOp` so the
//! `PodcastHostOpHandler` (running on the actor thread) can call platform
//! capabilities without the kernel naming podcast-domain nouns (D0).

use serde::{Deserialize, Serialize};

fn default_true() -> bool {
    true
}

use nmp_core::substrate::ActionModule;
use nmp_core::actor::ActorCommand;

use crate::discover_nostr::{nostr_discovery_identity, nostr_discovery_interest};

/// Wire enum for all `"podcast"` namespace actions.
///
/// `#[serde(tag = "op", rename_all = "snake_case")]` makes the JSON
/// discriminator the lowercase snake-case variant name:
/// `subscribe` → `{"op":"subscribe","feed_url":"..."}`.
///
/// Future actions (play, pause, seek, download, …) are added as new
/// variants here — no new ActionModule registrations needed.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum PodcastAction {
    Subscribe {
        feed_url: String,
    },
    /// Fetch and ingest a feed as a known podcast without marking it followed.
    /// Used by external episode listing and metadata hydration paths that need
    /// Rust to own the row/episodes but must not create a subscription.
    EnsurePodcast {
        feed_url: String,
    },
    /// Insert (or update) a podcast row from full caller-supplied metadata.
    /// `podcast_id` is the Swift-minted UUID so both stores agree on identity.
    /// `feed_url` distinguishes a feed-backed show (external-play placeholder)
    /// from a feed-less agent-owned / TTS show (absent). Idempotent on id — an
    /// enriched re-create updates the row in place. `visibility` is the
    /// canonical `NostrVisibility` snake_case string (`"public"` / `"private"`).
    /// `title_is_placeholder` marks a provisional feed-host fallback title.
    /// A feed-less podcast is just a podcast with no `feed_url`.
    CreatePodcast {
        podcast_id: String,
        title: String,
        #[serde(default)]
        description: String,
        #[serde(default)]
        author: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        feed_url: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        artwork_url: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        language: Option<String>,
        #[serde(default)]
        categories: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        visibility: Option<String>,
        #[serde(default)]
        title_is_placeholder: bool,
    },
    /// Insert (or update) an episode under a podcast. `podcast_id` / `episode_id`
    /// are the Swift-minted UUID strings. `enclosure_url` branches on scheme:
    /// a `file://` URL or bare absolute path → the audio is already on disk
    /// (Downloaded + local-path side-map); an `http(s)://` URL → a remote
    /// enclosure (NotDownloaded, fetched later by the download capability).
    /// `chapters` carry the parity fields; `transcript` is the flat episode
    /// transcript text; `image_url` overrides the per-episode artwork.
    AddEpisode {
        podcast_id: String,
        episode_id: String,
        title: String,
        enclosure_url: String,
        #[serde(default)]
        description: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration_secs: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        image_url: Option<String>,
        #[serde(default)]
        chapters: Vec<EpisodeChapterArg>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        transcript: Option<String>,
    },
    Unsubscribe {
        podcast_id: String,
    },
    /// Remove only the follow membership, keeping the podcast row and episodes
    /// as "known but unfollowed". A subsequent `Subscribe` with the same
    /// feed URL will mark_subscribed (no network fetch needed). Used by the
    /// ShowDetailView "Unsubscribe" button where the user may immediately
    /// re-follow without navigating away.
    Unfollow {
        podcast_id: String,
    },
    Refresh {
        podcast_id: String,
    },
    RefreshAll,
    SearchItunes {
        query: String,
    },
    /// Import an OPML 2.0 subscription list. `content` is the raw XML string
    /// (Swift reads the file on the platform side and forwards the text).
    /// The handler parses entries via `podcast_feeds::import_opml`, then
    /// fans out to `handle_subscribe` for each unique feed URL.
    ImportOpml {
        content: String,
    },
    /// Begin downloading the episode's enclosure to local storage.
    ///
    /// The host op handler looks up the episode's `enclosure_url` from the
    /// `PodcastStore`, or uses the `url` field if provided by the caller (iOS).
    /// Then dispatches `DownloadCommand::StartDownload` to the iOS
    /// `DownloadCapability`. The capability owns the `URLSessionDownloadTask`;
    /// once the report path wires up, `Completed` reports stamp `local_path`
    /// into the store, which the snapshot surfaces as `EpisodeSummary.download_path`.
    Download {
        episode_id: String,
        /// Optional enclosure URL passed directly from iOS. If provided, skips
        /// the store lookup (useful when the episode may not be indexed yet).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        url: Option<String>,
    },
    /// Begin downloading every currently eligible episode for one podcast.
    /// Rust owns the eligibility pass; the download queue keeps repeated
    /// dispatches idempotent for queued/in-flight episodes.
    DownloadPodcast {
        podcast_id: String,
    },
    /// Remove a previously downloaded episode from disk and clear the
    /// kernel-side `local_path` mapping.
    DeleteDownload {
        episode_id: String,
    },
    /// Begin downloading an on-device LLM model through the unified download
    /// queue (kind = `LocalModel`). User-initiated and direct: the `url` is
    /// always supplied by the shell (models aren't in the episode store), and
    /// this path deliberately bypasses the auto-download / deferred-wifi /
    /// subscription-revalidation machinery, which assumes episode ids.
    DownloadLocalModel {
        model_id: String,
        url: String,
    },
    FetchTranscript {
        episode_id: String,
    },
    /// Fetch and parse the Podcasting 2.0 chapters JSON for an episode.
    ///
    /// Self-gating in the handler: if the episode has no `chapters_url` or
    /// already has chapters loaded, the action is a `{"ok":true}` no-op.
    FetchChapters {
        episode_id: String,
    },
    /// NIP-F4 (`kind:10154`) podcast discovery through NMP's relay pool.
    ///
    /// `Claim` (`release: false`, the default) emits
    /// [`nmp_core::ActorCommand::EnsureInterest`]; the kernel opens the
    /// subscription through its own relays + the user's NIP-65 outbox relays
    /// (no relay URL — NMP routes automatically; the sweep is indexer-routed
    /// because `kind:10154` is sparse). `release: true` emits
    /// [`nmp_core::ActorCommand::DropInterestOwner`] to detach the consumer.
    ///
    /// Results arrive asynchronously via the registered
    /// [`crate::discover_nostr::NostrDiscoveryObserver`], which writes each
    /// inbound show onto the `nostr_results` snapshot slot — there is no
    /// synchronous result here (the iOS shell reads the projection).
    ///
    /// `consumer_id` ref-counts the interest by view instance: the form claims
    /// on appear and releases on disappear. This variant is special-cased in
    /// [`PodcastActionModule::execute`] — it is the one `podcast.*` action that
    /// emits an interest command instead of routing through the host-op
    /// handler, because emitting an `ActorCommand` requires the `send` closure
    /// that only `execute` carries.
    DiscoverNostr {
        consumer_id: String,
        /// `true` detaches this consumer (Release); `false`/absent attaches it
        /// (Claim). Flat boolean rather than a nested `tag = "op"` enum, which
        /// would collide with this enum's own `op` discriminator under serde.
        #[serde(default)]
        release: bool,
    },
    /// Subscribe to a feedless NIP-F4 show by its podcast Nostr pubkey.
    ///
    /// Routes through the host-op handler (unlike `DiscoverNostr`, which emits
    /// an `ActorCommand` directly). The handler calls `subscribe_nostr_episodes`
    /// to open a `kind:54` `EnsureInterest` via `push_interest_via_nmp`, then
    /// upserts a followed feedless show row in the store. Inbound episode events
    /// are delivered asynchronously via [`crate::nostr_episodes::NostrEpisodesObserver`]
    /// and surface on the snapshot with zero ffi/snapshot.rs changes.
    SubscribeNostr {
        /// Hex pubkey of the podcast's per-podcast Nostr key (`owner_pubkey_hex`
        /// from the discovered `kind:10154` show event).
        author_pubkey_hex: String,
        /// Optional show title for the feedless row (from the kind:10154 show).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        show_title: Option<String>,
    },
    /// Patch one or more fields on the kernel-side settings projection.
    ///
    /// All fields are `Option` so the iOS shell can patch a single setting
    /// at a time (e.g. only `has_completed_onboarding`) without round-tripping
    /// the full snapshot. `None` for a field means "leave existing value
    /// untouched" — replaces the legacy `updateSettings(Settings)` pattern
    /// which sent the full struct.
    UpdateSettings {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        has_completed_onboarding: Option<bool>,
    },
    /// Open the NIP-22 (kind 1111) comments subscription for
    /// `episode_id` and surface any matching events on the snapshot's
    /// `comments` field.
    ///
    /// Stub for this PR — the handler returns `{"ok":true}` and the
    /// projection layer leaves `comments` empty. The full relay
    /// subscription is tracked in `docs/BACKLOG.md`
    /// (`pr-episode-comments-relay-wiring`).
    FetchComments {
        episode_id: String,
    },
    /// Publish a kind-1111 NIP-22 comment anchored to `episode_id`.
    ///
    /// Stub for this PR — returns
    /// `{"ok":true,"status":"nostr_relay_pending"}` so iOS can render an
    /// optimistic confirmation while the relay-publish path is wired in
    /// a follow-up.
    PostComment {
        episode_id: String,
        content: String,
    },
    /// Set the per-podcast auto-download policy (D7). The kernel now owns the
    /// typed mode (Off / LatestN(n) / AllNew) and enforces the cap.
    ///
    /// ## Wire format (new)
    ///
    /// ```json
    /// {"op":"set_auto_download","podcast_id":"...","mode":"all_new","wifi_only":true}
    /// {"op":"set_auto_download","podcast_id":"...","mode":"latest_n","count":3}
    /// {"op":"set_auto_download","podcast_id":"...","mode":"off"}
    /// ```
    ///
    /// ## Back-compat shim
    ///
    /// A stale client or replayed action that only sends `enabled: bool`
    /// (and omits `mode`) is handled by the deserializer:
    /// `enabled: true` → `mode = "all_new"`, `enabled: false` → `mode = "off"`.
    /// `mode` takes precedence when present.
    ///
    /// Per D0: Rust owns the decision; iOS only renders the toggle
    /// state from the projection.
    /// `wifi_only` — when `true` (the default), auto-download only fires when
    /// the device is on Wi-Fi; when `false`, cellular downloads are allowed.
    /// Defaults to `true` when absent (backward-compatible with old dispatches
    /// that only sent `enabled`).
    SetAutoDownload {
        podcast_id: String,
        /// Typed mode. When present, overrides the legacy `enabled` field.
        /// One of `"off"`, `"all_new"`, `"latest_n"`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mode: Option<String>,
        /// Episode cap used with `mode = "latest_n"`. Ignored for other modes.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        count: Option<u32>,
        /// Legacy bool — kept for back-compat with stale clients that only send
        /// `enabled`. Ignored when `mode` is present.
        #[serde(default)]
        enabled: bool,
        #[serde(default = "default_true")]
        wifi_only: bool,
    },
    /// Toggle new-episode notifications for one podcast. Global notification
    /// permission/setting remains under `podcast.settings`; this per-show
    /// policy is applied by the Rust refresh handler before scheduling the
    /// native notification capability.
    SetPodcastNotificationsEnabled {
        podcast_id: String,
        enabled: bool,
    },
    /// Fetch the active account's NIP-02 (kind:3) follow list and surface
    /// the result on `PodcastUpdate.social`.
    ///
    /// Stub for this PR — the handler returns
    /// `{"ok":true,"status":"nostr_pending"}` so iOS can render a
    /// loading state while the NMP substrate contact store is wired
    /// into the projection layer in a follow-up
    /// (`pr-social-graph-nmp-store-wiring` in `docs/BACKLOG.md`).
    FetchContacts,
    /// Feature #44 — publish an agent-to-agent kind:1 note addressed to
    /// `recipient_pubkey_hex`, threaded with NIP-10 when `root_event_id`
    /// is set. Signs with the active identity and broadcasts to the relay.
    ///
    /// Returns `{"status":"published"|"signed","event_id":"..."}`. This is
    /// the public-note transport the matrix specifies for agent
    /// coordination — NIP-17 private DMs are an explicit non-goal.
    PublishAgentNote {
        recipient_pubkey_hex: String,
        content: String,
        /// NIP-10 root event ID — generates `["e", id, "", "root"]`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        root_event_id: Option<String>,
        /// NIP-10 inbound event ID — when different from root_event_id,
        /// generates an additional `["e", id, "", "reply"]` tag.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        inbound_event_id: Option<String>,
        /// NIP-72 channel-anchor a-tags (`["a", coord]`). Rust appends them
        /// verbatim — Swift never constructs tag arrays.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        root_a_tags: Vec<String>,
    },
    /// Feature #44 — subscribe to inbound kind:1 notes addressed to the
    /// active account (`#p` filter) and surface them on
    /// `PodcastUpdate.agent_notes`. Every inbound note is projected as
    /// untrusted until the kind:3 contact/trust gate is wired
    /// (`agent-to-agent-kind1` in BACKLOG).
    FetchAgentNotes,
    /// Toggle or set the "starred" / bookmarked flag on an episode.
    ///
    /// When `starred` is `None` the kernel flips the current value;
    /// when `Some(true|false)` it sets it explicitly. Persists alongside
    /// the subscription list in `podcasts.json` so bookmarks survive a
    /// restart. The updated flag surfaces on the next snapshot tick via
    /// `EpisodeSummary.starred`.
    StarEpisode {
        episode_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        starred: Option<bool>,
    },
    /// Drain and dispatch any downloads deferred because the device was on
    /// cellular when their feed refreshed (Wi-Fi-only shows). Called by iOS
    /// `NetworkCapability` when `ConnectivityChanged` reports `is_wifi: true`.
    DispatchDeferredWifiDownloads,
    /// Re-evaluate auto-download policy over the *current* library and queue
    /// each enabled show's most-recent undownloaded episodes (op
    /// `auto_download_evaluate`). Dispatched by iOS on cold start — where the
    /// foreground `RefreshAll` (and thus the fresh-feed auto-download path) is
    /// skipped on the first activation — so episodes still download without a
    /// manual pull-to-refresh.
    AutoDownloadEvaluate,
    /// Record (or clear) a batch of AI Inbox triage decisions (M4 / D7).
    ///
    /// iOS owns the triage *computation* (the LLM pass in `InboxTriageService`)
    /// and reports the whole pass here so Rust becomes the source of truth and
    /// the decisions ride the snapshot projection rather than living only in
    /// Swift state. Batched (one op per `applyTriageDecisions` pass, up to
    /// ~`candidateCap` rows) so a back-catalog triage doesn't fire one
    /// rev-bump + full-library re-encode per episode. Each row's `decision` is
    /// the raw `TriageDecision` rawValue (`"inbox"` / `"archived"`), or the
    /// sentinel `"none"` to clear a prior decision (user-rescue / re-triage).
    /// `rationale` is the one-line "Because …" text shown on the Home Inbox
    /// card for `.inbox` picks; `is_hero` promotes the row to the single hero
    /// pick of the pass. Stored in the `episode_triage` side-map and surfaced
    /// via `EpisodeSummary::{triage_decision, triage_is_hero, triage_rationale}`.
    SetEpisodeTriage {
        decisions: Vec<EpisodeTriagePatch>,
    },
    /// Mark a batch of episodes as covered by the RAG metadata index (M4 / D7).
    ///
    /// iOS's `EpisodeMetadataIndexer` / `TranscriptIngestService` embed the
    /// title+description (or transcript) chunk, then report the covered ids
    /// here so the `metadata_indexed` flag survives a feed refresh via the
    /// projection instead of the deleted preserved-state merge. Batched (one
    /// op for the whole backfill pass) so a large library doesn't fire one
    /// rev-bump + full-library re-encode per episode.
    MarkEpisodesMetadataIndexed {
        episode_ids: Vec<String>,
    },
    /// Report the transient transcript-ingestion status for an episode
    /// (M4 / D7).
    ///
    /// Rust derives `.ready` from the presence of the stored `transcript`
    /// field; it cannot observe the in-progress / failed states the iOS
    /// pipeline moves through. iOS reports them here so the
    /// `TranscribingInProgressView` copy + the Library "Transcribing" capsule
    /// keep their fidelity through projection passes. `status` is one of
    /// `"queued"` | `"fetching_publisher"` | `"transcribing"` | `"failed"` |
    /// `"none"` (clear). `message` carries the user-facing error text for
    /// `"failed"`. Stored in `transcript_status_overrides`; surfaced via
    /// `EpisodeSummary::{transcript_status, transcript_status_message}`.
    SetEpisodeTranscriptStatus {
        episode_id: String,
        /// `"queued"` | `"fetching_publisher"` | `"transcribing"` | `"failed"` | `"none"`.
        status: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
        /// Human-readable name of the STT service the iOS pipeline is using for
        /// this attempt (e.g. "ElevenLabs Scribe", "Apple Native (on-device)").
        /// Optional + back-compat: older callers omit it. When present it is
        /// surfaced as the `Service` detail on the `transcript.attempt` event so
        /// the Diagnostics log names *which* service is transcribing.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        provider: Option<String>,
    },
    /// Generate an AI summary for an episode (replaces the deleted Swift
    /// `LiveEpisodeSummarizerAdapter`).
    ///
    /// The handler reads the episode's title + description + cached transcript,
    /// spawns an off-actor Ollama call ([`crate::episode_summary_llm`]), stamps
    /// the result onto the episode's persisted `summary` field, and bumps `rev`
    /// so the projection surfaces it via `EpisodeSummary.summary`. Fire-and-
    /// forget at the dispatch level: returns `{"ok":true,"status":"summarizing"}`
    /// immediately. The iOS `summarize_episode` agent tool dispatches this then
    /// awaits the snapshot until `episode.summary` populates.
    SummarizeEpisode {
        episode_id: String,
    },
    /// Open the in-app feedback subscription through `nmp-feedback`. The module
    /// pushes a relay-pinned `OneShot` interest for kind:1 + kind:513 events
    /// bearing the app's project `["a"]` coord; results surface on
    /// `PodcastUpdate.feedback_events` and `feedback_threads`. No iOS relay
    /// socket.
    FetchFeedback,
    /// Sign + publish a feedback note (kind:1) to the feedback relay via NMP.
    /// Rust builds all tags (`["a",coord]`, `["t",category]`, the NIP-70 `["-"]`
    /// protected marker, and NIP-10 `["e",…,"root"]` / `["p",…]` for replies);
    /// Swift passes only semantic values. NMP signs with the active user signer
    /// and AUTHs the explicit-target write — no secret bytes in app code, no iOS
    /// relay socket. Roots omit `parent_event_id`/`reply_to_pubkey`.
    PublishFeedback {
        category: String,
        content: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent_event_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reply_to_pubkey: Option<String>,
    },
    /// Assign user-curated category labels to a podcast.
    /// Wire: `{"op":"set_podcast_user_categories","podcast_id":"<uuid>","categories":["AI","News"]}`
    /// An empty categories array clears all labels for the podcast.
    SetPodcastUserCategories {
        podcast_id: String,
        #[serde(default)]
        categories: Vec<String>,
    },
    /// Set the per-podcast transcription enabled flag.
    /// Wire: `{"op":"set_podcast_transcription_enabled","podcast_id":"<uuid>","enabled":false}`
    /// `enabled: true` removes the podcast from the disabled set (default);
    /// `enabled: false` inserts it.
    SetPodcastTranscriptionEnabled {
        podcast_id: String,
        enabled: bool,
    },
}

/// One chapter for an [`PodcastAction::AddEpisode`] op. `image_url` +
/// `source_episode_id` carry the parity fields the Swift TTS composer built on
/// `Episode.Chapter` (mid-play artwork swap + source-episode chip). They round
/// the kernel store, not just the wire, so the projected chapter is identical
/// to the pre-kernel build.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct EpisodeChapterArg {
    pub start_secs: f64,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_episode_id: Option<String>,
}

/// One row in a [`PodcastAction::SetEpisodeTriage`] batch.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct EpisodeTriagePatch {
    pub episode_id: String,
    /// `"inbox"` | `"archived"` | `"none"` (sentinel: clear).
    pub decision: String,
    #[serde(default)]
    pub is_hero: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
}

/// Single action module for the whole `"podcast"` namespace.
///
/// `execute` serializes the typed `PodcastAction` back to JSON and hands it
/// to the actor as `ActorCommand::DispatchHostOp`. The installed
/// `PodcastHostOpHandler` deserializes it, runs the op (HTTP capability call,
/// store write), and returns a `{"ok":true}` envelope. All policy lives in
/// the handler; the action module is pure routing.
pub struct PodcastActionModule;

impl ActionModule for PodcastActionModule {
    const NAMESPACE: &'static str = "podcast";

    type Action = PodcastAction;

    fn is_async_completing() -> bool {
        false
    }

    fn execute(
        &self,
        action: Self::Action,
        correlation_id: &str,
        send: &dyn Fn(ActorCommand),
    ) -> Result<(), String> {
        // `discover_nostr` is the one `podcast.*` action that drives an
        // interest subscription rather than a host-op. NMP core owns all relay
        // connections (D7): the kernel opens the `kind:10154` subscription
        // through its own relay pool on `EnsureInterest`, and inbound shows
        // arrive via `NostrDiscoveryObserver`. Emitting an `ActorCommand`
        // requires the `send` closure, which only `execute` carries — so it
        // cannot live in the host-op handler.
        if let PodcastAction::DiscoverNostr {
            consumer_id,
            release,
        } = &action
        {
            let identity = nostr_discovery_identity(consumer_id);
            if *release {
                send(ActorCommand::DropInterestOwner(identity));
            } else {
                send(ActorCommand::EnsureInterest {
                    identity,
                    interest: nostr_discovery_interest(),
                });
            }
            return Ok(());
        }

        crate::ffi::actions::dispatch_host_op(Self::NAMESPACE, &action, correlation_id, send)
    }

    fn decode_payload(
        bytes: &[u8],
    ) -> Option<Result<Self::Action, nmp_core::substrate::ActionPayloadDecodeError>> {
        crate::action_payload::decode_podcast_payload(bytes)
    }
}

#[cfg(test)]
#[path = "podcast_module_tests.rs"]
mod tests;
