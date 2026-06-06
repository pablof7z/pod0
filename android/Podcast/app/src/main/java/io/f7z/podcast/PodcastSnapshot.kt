package io.f7z.podcast

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

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
    /** Playback "Up Next" queue, front-first. Mirror of `PodcastUpdate.queue`. */
    val queue: List<EpisodeSummary> = emptyList(),
    /** AI-triaged inbox, highest-priority first. Mirror of `PodcastUpdate.inbox`. */
    val inbox: List<InboxItem> = emptyList(),
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

/**
 * Mirror of `apps/nmp-app-podcast/src/ffi/projections.rs::DownloadQueueSnapshot`.
 */
@Serializable
data class DownloadQueueSnapshot(
    val active: List<DownloadItemSnapshot> = emptyList(),
    @SerialName("queued_count") val queuedCount: Int = 0,
    @SerialName("completed_today") val completedToday: Int = 0,
)

/**
 * Mirror of `DownloadItemSnapshot`. `state` is a string discriminator
 * (`"active"` / `"queued"` / `"paused"` / `"failed"`).
 */
@Serializable
data class DownloadItemSnapshot(
    @SerialName("episode_id") val episodeId: String,
    /**
     * Enclosure URL the executor fetches. Projected by Rust
     * (`ffi/projections/download.rs::DownloadItemSnapshot.url`) so the
     * pull-model `DownloadCapability` can start the HTTP download straight
     * from a `"queued"` / `"active"` row while the kernel remains the only
     * download policy owner. Empty when the row predates the field.
     */
    val url: String = "",
    val progress: Float = 0.0f,
    val state: String,
    /**
     * Total file size in bytes once the server reports `Content-Length`.
     * `null` until the first HTTP response. Mirror of the Rust
     * `DownloadItemSnapshot.total_bytes` (`Option<u64>`); only ever present
     * for in-flight rows — completed downloads drop out of `active` entirely.
     */
    @SerialName("total_bytes") val totalBytes: Long? = null,
    val error: String? = null,
)

/**
 * Mirror of `ConversationsSnapshot` (named `AgentSnapshot` here to match
 * the field name `agent: AgentSnapshot?` used in `PodcastSnapshot`).
 */
@Serializable
data class AgentSnapshot(
    @SerialName("active_count") val activeCount: Int = 0,
    @SerialName("pending_approvals") val pendingApprovals: List<PendingApprovalSnapshot> = emptyList(),
    @SerialName("latest_conversation_id") val latestConversationId: String? = null,
)

/**
 * Mirror of `PendingApprovalSnapshot`. `requestedAt` is Unix seconds.
 */
@Serializable
data class PendingApprovalSnapshot(
    val id: String,
    val description: String,
    @SerialName("requested_at") val requestedAt: Long,
)

/**
 * Mirror of `VoiceState` (named `VoiceStateSnapshot` on the Kotlin side).
 */
@Serializable
data class VoiceStateSnapshot(
    @SerialName("is_speaking") val isSpeaking: Boolean = false,
    @SerialName("current_request_id") val currentRequestId: String? = null,
    @SerialName("current_voice_id") val currentVoiceId: String? = null,
)

/**
 * Mirror of `BriefingSnapshot`. `status` is one of `"pending"`,
 * `"generating"`, `"ready"`, `"delivered"`, `"failed"`.
 */
@Serializable
data class BriefingSnapshot(
    val status: String = "pending",
    @SerialName("segment_count") val segmentCount: Int = 0,
    @SerialName("next_scheduled_minutes") val nextScheduledMinutes: Int? = null,
    /** Optional human-readable schedule label ("Daily at 7:00 AM"). */
    @SerialName("schedule_label") val scheduleLabel: String? = null,
)

/**
 * Mirror of `WidgetSnapshot`. Per D7 the kernel decides what to surface;
 * the widget renders the empty state when fields are `null`.
 */
@Serializable
data class WidgetSnapshot(
    @SerialName("now_playing_episode_title") val nowPlayingEpisodeTitle: String? = null,
    @SerialName("now_playing_podcast_title") val nowPlayingPodcastTitle: String? = null,
    @SerialName("now_playing_artwork_url") val nowPlayingArtworkUrl: String? = null,
    @SerialName("is_playing") val isPlaying: Boolean = false,
    @SerialName("position_fraction") val positionFraction: Float = 0.0f,
    @SerialName("unplayed_count") val unplayedCount: Int = 0,
)

/**
 * Identity projection mirrored from `PodcastUpdate.active_account` (M1.A wire
 * target). The Kotlin side carries a forward-compatible shape so M13.C/D can
 * render an Identity screen even before the Rust serializer emits the field.
 */
@Serializable
data class AccountSummary(
    val npub: String,
    @SerialName("display_name") val displayName: String? = null,
    val mode: String = "local_key",
    @SerialName("picture_url") val pictureUrl: String? = null,
)

/**
 * One row of the library projection. Kept as the stable contract the
 * Compose UI compiles against even though the Rust serializer is still
 * on the M2.F stub.
 */
@Serializable
data class PodcastSummary(
    val id: String,
    val title: String,
    @SerialName("episode_count") val episodeCount: Int = 0,
    @SerialName("unplayed_count") val unplayedCount: Int = 0,
    @SerialName("artwork_url") val artworkUrl: String? = null,
    /**
     * RSS feed URL. Present for library rows and iTunes search results;
     * the key the search screen subscribes against (`{"op":"subscribe",
     * "feed_url":…}`). Mirror of `PodcastSummary.feed_url`.
     */
    @SerialName("feed_url") val feedUrl: String? = null,
    /** Podcast author / host. Mirror of `PodcastSummary.author`. */
    val author: String? = null,
    /** HTML-stripped podcast description. Mirror of `PodcastSummary.description`. */
    val description: String? = null,
    val episodes: List<EpisodeSummary> = emptyList(),
)

/**
 * Narrow episode row consumed by the Home "Recent Episodes" carousel and the
 * Show-detail list.
 */
@Serializable
data class EpisodeSummary(
    val id: String,
    val title: String,
    @SerialName("podcast_id") val podcastId: String? = null,
    @SerialName("podcast_title") val podcastTitle: String? = null,
    @SerialName("duration_secs") val durationSecs: Double? = null,
    @SerialName("artwork_url") val artworkUrl: String? = null,
    @SerialName("published_at") val publishedAt: Long? = null,
    /** Original RSS enclosure URL for streaming. Mirror of `EpisodeSummary.enclosure_url`. */
    @SerialName("enclosure_url") val enclosureUrl: String? = null,
    /** Episode show-notes / description (HTML — strip before rendering). */
    val description: String? = null,
    /** True once the user has listened to completion. */
    val played: Boolean = false,
    /** True when the episode is starred / bookmarked. */
    val starred: Boolean = false,
    /** On-disk path to the downloaded enclosure, or `null` when not downloaded. */
    @SerialName("download_path") val downloadPath: String? = null,
    /** Persisted resume position in seconds, or `null` for a fresh episode. */
    @SerialName("playback_position_secs") val playbackPositionSecs: Double? = null,
    /** Chapter markers. Mirror of `EpisodeSummary.chapters` (`Vec<ChapterSummary>`). */
    val chapters: List<ChapterSummary> = emptyList(),
    /** AI-assigned topic labels. Mirror of `EpisodeSummary.ai_categories`. */
    @SerialName("ai_categories") val aiCategories: List<String> = emptyList(),
    /** AI Inbox triage decision (`"inbox"` | `"archived"`), or `null` if untriaged. */
    @SerialName("triage_decision") val triageDecision: String? = null,
)

/**
 * One chapter marker on an episode. Mirror of the Rust
 * `ffi/projections/library.rs::ChapterSummary`.
 *
 * The wire field is `start_secs` (`f64`, **required**) — not the task-spec's
 * `start_time_secs`. `title` is a required `String` on the Rust side; defaulted
 * to empty here so a malformed row can't fail the whole snapshot decode.
 */
@Serializable
data class ChapterSummary(
    @SerialName("start_secs") val startSecs: Double = 0.0,
    @SerialName("end_secs") val endSecs: Double? = null,
    val title: String = "",
    @SerialName("image_url") val imageUrl: String? = null,
    val url: String? = null,
)

/**
 * Minimal slice of the Rust `ffi/projections/settings.rs::SettingsSnapshot`.
 *
 * The full Rust struct carries ~50 fields (model ids, credential sources, …).
 * The Android shell only needs playback-relevant settings today; the
 * `ignoreUnknownKeys = true` decoder drops the rest. When a screen needs more
 * fields, add them here against the verified snake_case wire names.
 */
@Serializable
data class SettingsSnapshot(
    @SerialName("default_playback_rate") val defaultPlaybackRate: Float = 1.0f,
    @SerialName("auto_delete_downloads_after_played") val autoDeleteDownloads: Boolean = false,
)

/**
 * One AI-triaged inbox row. Mirror of `ffi/projections/inbox.rs::InboxItem`.
 *
 * Field names follow the verified Rust projection — `episode_id`,
 * `episode_title`, `podcast_id`, `podcast_title`, `priority_score`,
 * `priority_reason` — **not** the task-spec's approximated `id`/`decision`/
 * `is_hero`/`score` shape (which would silently decode to empty).
 */
@Serializable
data class InboxItem(
    @SerialName("episode_id") val episodeId: String = "",
    @SerialName("episode_title") val episodeTitle: String = "",
    @SerialName("podcast_id") val podcastId: String = "",
    @SerialName("podcast_title") val podcastTitle: String = "",
    @SerialName("artwork_url") val artworkUrl: String? = null,
    @SerialName("published_at") val publishedAt: Long = 0,
    @SerialName("duration_secs") val durationSecs: Double? = null,
    @SerialName("priority_score") val priorityScore: Float = 0.0f,
    @SerialName("priority_reason") val priorityReason: String? = null,
    @SerialName("ai_categories") val aiCategories: List<String> = emptyList(),
)

/** Lazy JSON parser shared by the snapshot consumer. */
object SnapshotCodec {
    private val json = Json {
        ignoreUnknownKeys = true
        coerceInputValues = true
    }

    fun decode(raw: String?): PodcastSnapshot? =
        raw?.let { runCatching { json.decodeFromString<PodcastSnapshot>(it) }.getOrNull() }
}
