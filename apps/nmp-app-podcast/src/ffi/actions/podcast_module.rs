//! Compound podcast ActionModule — routes all `"podcast.*"` dispatches.
//!
//! Swift encodes every podcast action as `{"op":"<variant>", ...fields}`.
//! The `#[serde(tag = "op", rename_all = "snake_case")]` discriminator maps
//! the string `op` value to the enum variant. The module's `execute` body
//! forwards the whole action as `ActorCommand::DispatchHostOp` so the
//! `PodcastHostOpHandler` (running on the actor thread) can call platform
//! capabilities without the kernel naming podcast-domain nouns (D0).

use serde::{Deserialize, Serialize};

fn default_true() -> bool { true }

use nmp_core::substrate::ActionModule;
use nmp_core::ActorCommand;

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
    Subscribe { feed_url: String },
    Unsubscribe { podcast_id: String },
    Refresh { podcast_id: String },
    RefreshAll,
    SearchItunes { query: String },
    /// Import an OPML 2.0 subscription list. `content` is the raw XML string
    /// (Swift reads the file on the platform side and forwards the text).
    /// The handler parses entries via `podcast_feeds::import_opml`, then
    /// fans out to `handle_subscribe` for each unique feed URL.
    ImportOpml { content: String },
    /// Begin downloading the episode's enclosure to local storage.
    ///
    /// The host op handler looks up the episode's `enclosure_url` from the
    /// `PodcastStore`, then dispatches `DownloadCommand::StartDownload` to
    /// the iOS `DownloadCapability`. The capability owns the
    /// `URLSessionDownloadTask`; once the report path wires up, `Completed`
    /// reports stamp `local_path` into the store, which the snapshot
    /// surfaces as `EpisodeSummary.download_path`.
    Download { episode_id: String },
    /// Remove a previously downloaded episode from disk and clear the
    /// kernel-side `local_path` mapping.
    DeleteDownload { episode_id: String },
    FetchTranscript { episode_id: String },
    /// Fetch and parse the Podcasting 2.0 chapters JSON for an episode.
    ///
    /// Self-gating in the handler: if the episode has no `chapters_url` or
    /// already has chapters loaded, the action is a `{"ok":true}` no-op.
    FetchChapters { episode_id: String },
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
    /// Compose a fresh daily briefing on demand. No fields — the handler
    /// reads the current library snapshot and the configured schedule to
    /// pick source episodes.
    ///
    /// M9.A stub: the handler currently flips a `generating` status into
    /// the snapshot and returns `{"ok":true,"status":"generating"}`. The
    /// LLM composer + audio stitching wiring lands in M9.B; this variant
    /// reserves the action-dispatch path so the iOS button can be wired
    /// against a stable contract today.
    GenerateBriefing,
    /// Open the NIP-22 (kind 1111) comments subscription for
    /// `episode_id` and surface any matching events on the snapshot's
    /// `comments` field.
    ///
    /// Stub for this PR — the handler returns `{"ok":true}` and the
    /// projection layer leaves `comments` empty. The full relay
    /// subscription is tracked in `docs/BACKLOG.md`
    /// (`pr-episode-comments-relay-wiring`).
    FetchComments { episode_id: String },
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
    /// Toggle the per-podcast auto-download policy. When `enabled` is
    /// `true`, subsequent `handle_refresh` calls will dispatch
    /// `DownloadCommand::StartDownload` for every freshly-discovered
    /// episode (matched by `guid` against the previously-known set).
    /// When `false`, the policy is removed and refreshes go back to
    /// only surfacing new episodes in the snapshot.
    ///
    /// Per D0: Rust owns the decision; iOS only renders the toggle
    /// state from the projection.
    /// `wifi_only` — when `true` (the default), auto-download only fires when
    /// the device is on Wi-Fi; when `false`, cellular downloads are allowed.
    /// Defaults to `true` when absent (backward-compatible with old dispatches
    /// that only sent `enabled`).
    SetAutoDownload {
        podcast_id: String,
        enabled: bool,
        #[serde(default = "default_true")]
        wifi_only: bool,
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        root_event_id: Option<String>,
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
    SetEpisodeTriage { decisions: Vec<EpisodeTriagePatch> },
    /// Mark a batch of episodes as covered by the RAG metadata index (M4 / D7).
    ///
    /// iOS's `EpisodeMetadataIndexer` / `TranscriptIngestService` embed the
    /// title+description (or transcript) chunk, then report the covered ids
    /// here so the `metadata_indexed` flag survives a feed refresh via the
    /// projection instead of the deleted preserved-state merge. Batched (one
    /// op for the whole backfill pass) so a large library doesn't fire one
    /// rev-bump + full-library re-encode per episode.
    MarkEpisodesMetadataIndexed { episode_ids: Vec<String> },
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
    SummarizeEpisode { episode_id: String },
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
        if let PodcastAction::DiscoverNostr { consumer_id, release } = &action {
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

        let action_json =
            serde_json::to_string(&action).map_err(|e| e.to_string())?;
        send(ActorCommand::DispatchHostOp {
            action_json,
            correlation_id: correlation_id.to_owned(),
        });
        Ok(())
    }
}

#[cfg(test)]
#[path = "podcast_module_tests.rs"]
mod tests;
