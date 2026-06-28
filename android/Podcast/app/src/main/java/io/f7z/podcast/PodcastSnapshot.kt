package io.f7z.podcast

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement

/**
 * Kotlin mirror of `apps/nmp-app-podcast/src/ffi/snapshot.rs::PodcastUpdate`.
 *
 * Every field on the Rust struct has a matching property here so the Compose
 * shell can render any state the kernel projects. New fields land on both
 * sides simultaneously. The canonical wire shape lives in
 * `apps/nmp-app-podcast/src/ffi/snapshot.rs`.
 *
 * Every field below this line is optional / defaulted so the existing payload
 * still decodes. As later milestones (M1, M2.A, M3.A, M9.A, …) extend
 * `PodcastUpdate` in Rust, the matching field on this struct starts carrying
 * real data with **zero** Kotlin-side changes.
 *
 * The `Json` decoder is configured with `ignoreUnknownKeys = true` so an older
 * Android build can still decode a newer kernel snapshot (forward compat).
 *
 * Wire-shape source of truth: `apps/nmp-app-podcast/src/ffi/snapshot.rs`
 * (`PodcastUpdate`) + `apps/nmp-app-podcast/src/ffi/projections.rs`.
 *
 * **Doctrine — D5 / D7:**
 *  * The kernel decides what to surface; this struct is pure decode +
 *    render scaffolding. No Kotlin-side derivations beyond `null` checks.
 *  * `Option<T>` on the Rust side becomes nullable here, with `null`
 *    defaults so missing JSON fields decode cleanly (forward compat).
 */
@Serializable
data class PodcastSnapshot(
    val running: Boolean = false,
    val rev: Long = 0,
    @SerialName("schema_version") val schemaVersion: Int = 0,
    /** Active player projection, `null` when no episode is loaded. */
    @SerialName("now_playing") val nowPlaying: NowPlayingState? = null,
    /** Active download queue, `null` until the first enqueue. */
    val downloads: DownloadQueueSnapshot? = null,
    /** Agent-chat projection, `null` until the first turn. */
    val agent: AgentSnapshot? = null,
    /** Briefing scheduler state, `null` until first scheduler touch. */
    val briefing: BriefingSnapshot? = null,
    /** Voice/TTS session state, `null` while idle. */
    val voice: VoiceStateSnapshot? = null,
    /** Widget/Live-Activity projection, `null` until populated. */
    val widget: WidgetSnapshot? = null,
    /** Transient toast the kernel wants the host to surface, or `null`. */
    val toast: String? = null,
    /** Active identity (M1.A — `active_account` snapshot field). `null` when nobody is signed in. */
    @SerialName("active_account") val activeAccount: AccountSummary? = null,
    /**
     * Library rows. Emitted by the kernel under the `library` wire key today
     * (M2.F stub) and will migrate to `podcasts` in M2.A. The Compose UI reads
     * [subscriptions] which prefers the new field when present.
     */
    val library: List<PodcastSummary> = emptyList(),
    /**
     * Forward-compat alias for the M2.A `PodcastUpdate.podcasts` projection.
     * Empty until M2.A's FFI wiring lands; UI code should read [subscriptions]
     * which transparently falls back to [library].
     */
    @SerialName("podcasts") val podcasts: List<PodcastSummary> = emptyList(),
    /**
     * iTunes/RSS directory search results, populated by dispatching the
     * `{"op":"search_itunes","query":…}` action on the `podcast` namespace.
     * Mirror of `PodcastUpdate.search_results` (a `Vec<PodcastSummary>`).
     * Wire key is snake_case, so the explicit `@SerialName` is load-bearing —
     * kotlinx does not auto-convert.
     */
    @SerialName("search_results") val searchResults: List<PodcastSummary> = emptyList(),
    /**
     * Playback / app settings projection. Mirror of `PodcastUpdate.settings`.
     * The Rust side `skip_serializing_if = "is_default"`, so this key is
     * **absent** from the wire whenever settings equal the fresh-install
     * default — hence nullable here. Read with a `?: default` fallback.
     */
    val settings: SettingsSnapshot? = null,
    /** Rust-owned local notes projection. */
    val notes: List<NoteSummary> = emptyList(),
    /** Rust-owned user-curated friends projection. */
    val friends: List<FriendSummary> = emptyList(),
    /** Playback "Up Next" queue, front-first. Mirror of `PodcastUpdate.queue`. */
    val queue: List<EpisodeSummary> = emptyList(),
    /** AI-triaged inbox, highest-priority first. Mirror of `PodcastUpdate.inbox`. */
    val inbox: List<InboxItem> = emptyList(),
    /**
     * `true` while the background LLM triage pass is running.
     * Mirror of `PodcastUpdate.inbox_triage_in_progress`. Drives the shimmer
     * indicator in the Inbox screen.
     */
    @SerialName("inbox_triage_in_progress") val inboxTriageInProgress: Boolean = false,
    /**
     * Unix seconds for the latest completed inbox triage pass.
     * Mirror of `PodcastUpdate.inbox_last_triaged_at`.
     */
    @SerialName("inbox_last_triaged_at") val inboxLastTriagedAt: Long? = null,
    /** Agent-scheduled task rows. Mirror of `PodcastUpdate.agent_tasks`. */
    @SerialName("agent_tasks") val agentTasks: List<AgentTaskSummary> = emptyList(),
    /**
     * Raw feedback events cached by the Rust feedback runtime. Android renders
     * [feedbackThreads]; this remains decoded for parity/debug surfaces only.
     */
    @SerialName("feedback_events") val feedbackEvents: List<JsonElement> = emptyList(),
    /** Resolved feedback threads emitted by `nmp-feedback`. */
    @SerialName("feedback_threads") val feedbackThreads: List<FeedbackThreadDto> = emptyList(),
    /**
     * AI-curated picks rail. Mirror of `PodcastUpdate.picks` —
     * `Vec<AgentPickSummary>` projected by `picks_handler.rs`. Populated by
     * the heuristic immediately on first library load, then re-scored by the
     * LLM pass. Rides the `podcast.misc` domain frame.
     */
    val picks: List<AgentPickSummary> = emptyList(),
    /**
     * User-saved audio clips. Mirror of `PodcastUpdate.clips` —
     * `Vec<ClipSummary>` projected by `clip_handler::project_clips`.
     * Rides the `podcast.misc` domain frame. Empty until the user creates
     * the first clip. Newest-first ordering is applied by the UI at render
     * time (kernel emits in insertion order, same as iOS).
     */
    val clips: List<ClipSummary> = emptyList(),
    /**
     * NIP-10-threaded Nostr conversations, newest-first by last_activity.
     * Mirror of `SocialDomainFrame.nostrConversations`. Rides the
     * `podcast.social` domain frame. Empty until the kernel has indexed
     * at least one conversation thread.
     */
    @SerialName("nostr_conversations") val nostrConversations: List<NostrConversationDto> = emptyList(),
    /**
     * NIP-02 follow list (kind:3), projected by the `podcast.social` domain frame.
     *
     * Rides the same atomic co-emit as `nostrConversations` — both clear together
     * on a social tombstone (account switch). Empty until the kernel has fetched
     * the active account's follow list.
     *
     * NOT a `@SerialName` field — populated by [SnapshotCodec.mergeFrames] from
     * `SocialDomainFrame.social.following`. Profile hydration (display name /
     * avatar) is slice-2 work; this slice renders npub stubs only.
     */
    val following: List<ContactSummaryDto> = emptyList(),
    /**
     * Kernel-resolved Nostr profiles keyed by hex pubkey.
     *
     * Populated by the NMP kernel from `projections["resolved_profiles"]` on
     * every push frame where claimed profiles have been resolved (T114
     * reference-first profile resolution). Merging is additive: entries are
     * never removed from this map mid-session. Conversation screens use this
     * map to show real names and avatars instead of shortHex fallbacks.
     *
     * NOT a `@SerialName` field — populated by [SnapshotCodec.mergeFrames]
     * directly from the top-level NMP `projections["resolved_profiles"]` key.
     * It does not ride a `podcast.*` domain sidecar.
     */
    val resolvedProfiles: Map<String, ResolvedProfile> = emptyMap(),
) {
    /**
     * Effective subscription list — prefer the new `podcasts` projection, fall
     * back to the M2.F `library` field if the kernel hasn't migrated yet.
     */
    val subscriptions: List<PodcastSummary>
        get() = if (podcasts.isNotEmpty()) podcasts else library
}

/**
 * Mirror of `apps/nmp-app-podcast/src/player/state.rs::PlayerState` (M13.C+D name).
 *
 * Used by `HomeScreen.NowPlayingCard` and `PlayerScreen`. Fields use snake_case
 * on the wire because the iOS `Codable` decoder — and the Rust struct itself —
 * speaks snake_case JSON.
 */
@Serializable
data class NowPlayingState(
    @SerialName("episode_id") val episodeId: String? = null,
    @SerialName("podcast_id") val podcastId: String? = null,
    @SerialName("episode_title") val episodeTitle: String? = null,
    @SerialName("podcast_title") val podcastTitle: String? = null,
    @SerialName("artwork_url") val artworkUrl: String? = null,
    @SerialName("position_secs") val positionSecs: Double = 0.0,
    @SerialName("duration_secs") val durationSecs: Double = 0.0,
    @SerialName("is_playing") val isPlaying: Boolean = false,
    val speed: Float = 1.0f,
    val volume: Float = 1.0f,
    @SerialName("sleep_timer_remaining_secs") val sleepTimerRemainingSecs: Long? = null,
    @SerialName("buffering_fraction") val bufferingFraction: Float? = null,
    @SerialName("last_error") val lastError: String? = null,
)

// SnapshotCodec lives in DomainFrames.kt — see that file for both the
// cold-start pull decoder and the per-domain push-frame merge path.
