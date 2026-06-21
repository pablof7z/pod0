package io.f7z.podcast

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/**
 * Mirror of `ffi/projections/agent.rs::AgentSnapshot`.
 *
 * Holds the full ordered transcript plus an `isBusy` flag used to disable
 * the send button and render the typing indicator while the kernel LLM loop
 * is running. `messages` is oldest-first; the UI renders it directly.
 *
 * This replaces the earlier (stale) `ConversationsSnapshot`-shaped mirror
 * that exposed `activeCount`/`latestConversationId` — those fields belong
 * to the multi-conversation `ConversationsSnapshot` struct, NOT the
 * single-thread agent-chat `AgentSnapshot` the `podcast.misc` frame carries.
 */
@Serializable
data class AgentSnapshot(
    /** Ordered transcript, oldest message first. */
    val messages: List<AgentMessageSummary> = emptyList(),
    /**
     * `true` while the kernel is producing a response. UI disables send
     * and shows a typing indicator; clears when the reply is committed.
     */
    @SerialName("is_busy") val isBusy: Boolean = false,
)

/**
 * One message in the agent-chat transcript.
 *
 * Mirror of `ffi/projections/agent.rs::AgentMessageSummary`.
 * `role` is `"user"` or `"assistant"`. `isGenerating` is `true` while
 * the assistant placeholder is still being composed.
 * `createdAt` is Unix seconds (epoch).
 */
@Serializable
data class AgentMessageSummary(
    val id: String,
    val role: String,
    val content: String,
    @SerialName("created_at") val createdAt: Long = 0L,
    @SerialName("is_generating") val isGenerating: Boolean = false,
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
 * Mirror of `ffi/projections/agent.rs::AgentTaskSummary`. The Android shell
 * renders user-facing intent metadata and mutates rows through the
 * `podcast.tasks` action namespace. Internal dispatch namespace/body JSON is
 * intentionally not part of this public projection.
 */
@Serializable
data class AgentTaskSummary(
    val id: String = "",
    val title: String = "",
    val description: String? = null,
    @SerialName("intent_type") val intentType: String = "custom",
    @SerialName("intent_label") val intentLabel: String = "Custom task",
    @SerialName("intent_detail") val intentDetail: String? = null,
    val schedule: String = "",
    @SerialName("next_run_at") val nextRunAt: Long? = null,
    @SerialName("last_run_at") val lastRunAt: Long? = null,
    val status: String = "pending",
    @SerialName("is_enabled") val isEnabled: Boolean = false,
)

/**
 * One AI-curated pick row surfaced via `PodcastUpdate.picks` (podcast.misc domain).
 *
 * Mirror of `ffi/projections/agent.rs::AgentPickSummary`. Fields are
 * pre-resolved (podcast title + artwork denormalized) so the Home picks rail can
 * render without a second snapshot lookup per row.
 *
 * Wire shape verified against the Rust struct — all snake_case keys are
 * load-bearing; kotlinx does not auto-convert. `pick_score` is `0.0..=1.0`.
 * Optional fields (`artwork_url`, `duration_secs`) mirror Rust `Option<T>`.
 */
@Serializable
data class AgentPickSummary(
    @SerialName("episode_id") val episodeId: String = "",
    @SerialName("episode_title") val episodeTitle: String = "",
    @SerialName("podcast_id") val podcastId: String = "",
    @SerialName("podcast_title") val podcastTitle: String = "",
    @SerialName("artwork_url") val artworkUrl: String? = null,
    @SerialName("published_at") val publishedAt: Long = 0,
    @SerialName("duration_secs") val durationSecs: Double? = null,
    @SerialName("pick_reason") val pickReason: String = "",
    @SerialName("pick_score") val pickScore: Float = 0.0f,
)

/** One reply row in a resolved feedback thread. */
@Serializable
data class FeedbackReplyDto(
    @SerialName("event_id") val eventId: String = "",
    @SerialName("author_pubkey") val authorPubkey: String = "",
    val content: String = "",
    @SerialName("created_at") val createdAt: Long = 0,
)

/** Resolved feedback thread projected by `nmp-feedback`. */
@Serializable
data class FeedbackThreadDto(
    @SerialName("event_id") val eventId: String = "",
    @SerialName("author_pubkey") val authorPubkey: String = "",
    val category: String = "bug",
    val content: String = "",
    @SerialName("created_at") val createdAt: Long = 0,
    val title: String? = null,
    val summary: String? = null,
    @SerialName("status_label") val statusLabel: String? = null,
    val replies: List<FeedbackReplyDto> = emptyList(),
)
