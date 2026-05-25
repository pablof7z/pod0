package io.f7z.podcast

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

/**
 * Mirror of the JSON the Rust `nmp_app_podcast_snapshot` entry point emits.
 *
 * Today (post-M2.F) the legacy stub payload is still
 *
 *   ```json
 *   {"running":true,"rev":0,"schema_version":1}
 *   ```
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
 */
@Serializable
data class PodcastSnapshot(
    val running: Boolean = false,
    val rev: Long = 0,
    @SerialName("schema_version") val schemaVersion: Int = 0,
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
    /** Currently-loaded player projection (M3.A); `null` when nothing is playing. */
    @SerialName("now_playing") val nowPlaying: NowPlayingState? = null,
    /** Briefing scheduler state (M9.A); `null` when the scheduler has never been touched. */
    val briefing: BriefingSnapshot? = null,
    /** Active identity (M1.A — `active_account` snapshot field). `null` when nobody is signed in. */
    @SerialName("active_account") val activeAccount: AccountSummary? = null,
) {
    /**
     * Effective subscription list — prefer the new `podcasts` projection, fall
     * back to the M2.F `library` field if the kernel hasn't migrated yet.
     */
    val subscriptions: List<PodcastSummary>
        get() = if (podcasts.isNotEmpty()) podcasts else library
}

/**
 * One row of the future library projection. Matches the shape the iOS shell
 * already decodes from `LibraryDisplayProjection`. Kept here so the Compose
 * UI compiles against a stable contract even though the Rust serializer is
 * still emitting stubs.
 *
 * `artworkUrl` and `episodes` are forward-compatible: empty / null today,
 * populated as M2.A / M2.B projections ship.
 */
@Serializable
data class PodcastSummary(
    val id: String,
    val title: String,
    @SerialName("episode_count") val episodeCount: Int = 0,
    @SerialName("unplayed_count") val unplayedCount: Int = 0,
    @SerialName("artwork_url") val artworkUrl: String? = null,
    /** Recent episodes for this show (M2.A windowed projection). Empty today. */
    val episodes: List<EpisodeSummary> = emptyList(),
)

/**
 * Narrow episode row consumed by the Home "Recent Episodes" carousel and the
 * Show-detail list. Mirrors the iOS `EpisodeSummary` Codable shape. Optional
 * fields are skipped on the wire when absent (`serde::skip_serializing_if`).
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
)

/**
 * Mirror of the Rust `crate::player::PlayerState` projection (M3.A).
 *
 * Every field is optional / defaulted because today's kernel emits `null`
 * for the whole `now_playing` slot when nothing is loaded — the struct only
 * appears in the snapshot when an episode has been queued.
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
)

/**
 * Mirror of `nmp_app_podcast::ffi::projections::BriefingSnapshot`. M9.A wires
 * the producer side; the UI renders the next-scheduled slot + status badge.
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
 * Identity projection mirrored from `PodcastUpdate.active_account` (M1.A wire
 * target — see `Plans/nmp-migration/04-snapshot.md`). The Kotlin side carries
 * a forward-compatible shape so M13.C/D can render an Identity screen even
 * before the Rust serializer emits the field.
 *
 * `mode` is the auth mode the user is currently in (`"local_key"` /
 * `"bunker"`); the Compose `ModeBadge` switches on this string.
 */
@Serializable
data class AccountSummary(
    val npub: String,
    @SerialName("display_name") val displayName: String? = null,
    val mode: String = "local_key",
    @SerialName("picture_url") val pictureUrl: String? = null,
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
