package io.f7z.podcast.ui

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/**
 * `podcast.inbox` namespace payloads (Tier-2 feature).
 *
 * Verified against `apps/nmp-app-podcast/src/ffi/actions/inbox_module.rs`
 * `InboxAction` enum. Wire discriminator: `#[serde(tag = "op", rename_all = "snake_case")]`.
 *
 * Split from `ActionDispatcher.kt` to keep that file under the 500-line hard
 * limit (AGENTS.md).
 */

/** Force-retrigger triage scoring. Bumps rev so the next snapshot tick rebuilds the inbox field. */
@Serializable
data class InboxTriagePayload(val op: String = "triage")

/** Session-dismiss an episode from the inbox (in-memory; clears on cold restart). */
@Serializable
data class InboxDismissPayload(
    @SerialName("episode_id") val episodeId: String,
    val op: String = "dismiss",
)

/** Mark an episode as listened (persists through the store, drops from inbox). */
@Serializable
data class InboxMarkListenedPayload(
    @SerialName("episode_id") val episodeId: String,
    val op: String = "mark_listened",
)

// ── `podcast` transcript payloads ─────────────────────────────────────────
//
// Verified against `apps/nmp-app-podcast/src/ffi/actions/podcast_module.rs`
// `PodcastAction::FetchTranscript`.

/** Fetch and parse the transcript for an episode (publisher URL or on-device STT). */
@Serializable
data class FetchTranscriptPayload(
    @SerialName("episode_id") val episodeId: String,
    val op: String = "fetch_transcript",
)
