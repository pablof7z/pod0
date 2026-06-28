package io.f7z.podcast

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/**
 * Identity projection mirrored from `PodcastUpdate.active_account`.
 * The Rust backend omits `activeAccount` entirely unless it can include both
 * the display npub and canonical hex account id.
 */
@Serializable
data class AccountSummary(
    val npub: String,
    @SerialName("pubkey_hex") val pubkeyHex: String,
    val fingerprint: String? = null,
    @SerialName("display_name") val displayName: String? = null,
    val mode: String = "local_key",
    @SerialName("picture_url") val pictureUrl: String? = null,
    val name: String? = null,
    val about: String? = null,
)

/** Local note row projected from Rust-owned NotesState. */
@Serializable
data class NoteSummary(
    val id: String,
    val text: String,
    val kind: String = "free",
    val target: NoteTargetSummary? = null,
    @SerialName("created_at") val createdAt: Long = 0,
    val deleted: Boolean = false,
    val author: String = "user",
)

/** Local note target. `type` is `"episode"`, `"podcast"`, `"friend"`, or `"note"`. */
@Serializable
data class NoteTargetSummary(
    val type: String,
    @SerialName("episode_id") val episodeId: String? = null,
    @SerialName("position_secs") val positionSecs: Double? = null,
    @SerialName("podcast_id") val podcastId: String? = null,
    @SerialName("friend_id") val friendId: String? = null,
    @SerialName("note_id") val noteId: String? = null,
)

/** User-curated friend row projected from Rust-owned FriendsState. */
@Serializable
data class FriendSummary(
    val id: String,
    @SerialName("display_name") val displayName: String,
    @SerialName("pubkey_hex") val pubkeyHex: String,
    @SerialName("added_at") val addedAt: Long = 0,
    @SerialName("avatar_url") val avatarUrl: String? = null,
    val about: String? = null,
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
    /**
     * User-curated category labels assigned to this podcast. Mirror of
     * `PodcastSummary.user_categories`. Default `emptyList()` keeps decode
     * safe for snapshots that predate the field (D5 omits it when empty).
     */
    @SerialName("user_categories") val userCategories: List<String> = emptyList(),
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
    /**
     * Publisher-provided transcript URL from the Podcasting 2.0
     * `<podcast:transcript>` tag. Non-null signals a transcript is available
     * to fetch via `podcast` `{"op":"fetch_transcript","episode_id":"..."}`.
     * Mirror of `EpisodeSummary.transcript_url`.
     */
    @SerialName("transcript_url") val transcriptUrl: String? = null,
    /**
     * Raw plain-text transcript, populated after a `fetch_transcript` action.
     * Mirror of `EpisodeSummary.transcript`.
     */
    val transcript: String? = null,
    /**
     * Structured transcript entries with timestamps.
     * Mirror of `EpisodeSummary.transcript_entries` (`Vec<TranscriptEntry>`).
     */
    @SerialName("transcript_entries") val transcriptEntries: List<TranscriptEntry> = emptyList(),
    /**
     * Transient transcript-ingestion status: `""` | `"queued"` | `"fetching_publisher"` |
     * `"transcribing"` | `"failed"`. Empty string means idle.
     * Mirror of `EpisodeSummary.transcript_status`.
     */
    @SerialName("transcript_status") val transcriptStatus: String = "",
    /**
     * User-facing error detail when `transcript_status == "failed"`.
     * Mirror of `EpisodeSummary.transcript_status_message`.
     */
    @SerialName("transcript_status_message") val transcriptStatusMessage: String? = null,
    /**
     * Ad-break intervals detected by the kernel's LLM ad-skip detector.
     * Per D5 the field is omitted from the wire when empty. The player renders
     * these as tinted regions on the progress bar; the kernel handles the
     * actual seek-past via `PlayerActor` when `auto_skip_ads_enabled` is true.
     * Mirror of `EpisodeSummary.ad_segments` (`Vec<AdSegment>`).
     */
    @SerialName("ad_segments") val adSegments: List<AdSegment> = emptyList(),
)

/**
 * Ad-break segment for an episode. Mirror of the Rust
 * `podcast_core::AdSegment` struct (re-exported via `player/ad_segments.rs`).
 *
 * Wire shape verified against `apps/podcast-core/src/types/ad_segment.rs`:
 *  - `id`: UUID serialised as a hyphenated string.
 *  - `start_secs` / `end_secs`: `f64` seconds.
 *  - `kind`: one of `"preroll"` | `"midroll"` | `"postroll"` (`snake_case`).
 *
 * Used by `EpisodeSummary.ad_segments` (per D5, field omitted when empty).
 * The player renders these as tinted regions on the progress bar; the kernel
 * handles the actual skip seek via `PlayerActor` when `auto_skip_ads_enabled`.
 */
@Serializable
data class AdSegment(
    val id: String = "",
    @SerialName("start_secs") val startSecs: Double = 0.0,
    @SerialName("end_secs") val endSecs: Double = 0.0,
    val kind: String = "midroll",
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
 * One time-stamped transcript segment. Mirror of the Rust
 * `ffi/projections/library.rs::TranscriptEntry`.
 *
 * `speaker` is optional (not all ingestors emit per-segment speaker labels).
 * `endSecs` is optional (some ingestors don't provide segment end times).
 */
@Serializable
data class TranscriptEntry(
    @SerialName("start_secs") val startSecs: Double = 0.0,
    @SerialName("end_secs") val endSecs: Double? = null,
    val speaker: String? = null,
    val text: String = "",
)
