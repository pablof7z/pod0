package io.f7z.podcast.ui

import io.f7z.podcast.KernelBridge
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

/**
 * Kernel action wire contract for the Android shell.
 *
 * **The wire model is `(namespace, op-tagged body)` — not flat action ids.**
 *
 * The Rust kernel registers exactly two podcast-domain `ActionModule`
 * namespaces:
 *
 *  * `"podcast"`        — subscribe, unsubscribe, refresh_all, search_itunes,
 *                         download, delete_download, star_episode, …
 *  * `"podcast.player"` — play, pause, resume, seek, set_speed,
 *                         set_sleep_timer, stop, …
 *  * `"podcast.queue"`  — add_next, add_last, remove, clear.
 *
 * Each action is encoded as `{"op":"<variant>", …fields}` and the Rust
 * `#[serde(tag = "op", rename_all = "snake_case")]` discriminator routes it.
 * `KernelBridge.dispatchAction(namespace, body)` → `nmp_app_dispatch_action`
 * forwards `(namespace, body)` straight into that registry, exactly like the
 * iOS `kernel.dispatch(namespace:body:)` reference in
 * `App/Sources/Bridge/AppStateStore+KernelActions.swift`.
 *
 * Source of truth verified against:
 *  * `apps/nmp-app-podcast/src/ffi/actions/podcast_module.rs` (PodcastAction)
 *  * `apps/nmp-app-podcast/src/ffi/actions/player_module.rs`  (PlayerAction)
 *  * `apps/nmp-app-podcast/src/android.rs` (nativeDispatchAction)
 *
 * NOTE: an earlier demo passed the *dotted op path* (e.g. `"podcast.player.play"`)
 * as the namespace argument. That string is not a registered namespace, so
 * those dispatches never reached the kernel. This file fixes the contract;
 * payloads carry their `op` as a defaulted field so `encodeDefaults = true`
 * emits the discriminator.
 */
object PodcastNamespace {
    const val PODCAST = "podcast"
    const val PLAYER = "podcast.player"
    const val QUEUE = "podcast.queue"
    const val SETTINGS = "podcast.settings"
}

// ── `podcast` namespace payloads ──────────────────────────────────────────
//
// Every payload pins its `op` discriminator as a defaulted field so callers
// never pass it by hand and `Json { encodeDefaults = true }` serializes it.

@Serializable
data class SubscribePayload(
    @SerialName("feed_url") val feedUrl: String,
    val op: String = "subscribe",
)

@Serializable
data class UnsubscribePayload(
    @SerialName("podcast_id") val podcastId: String,
    val op: String = "unsubscribe",
)

@Serializable
data class SearchPayload(
    val query: String,
    val op: String = "search_itunes",
)

@Serializable
data class RefreshAllPayload(val op: String = "refresh_all")

@Serializable
data class DownloadStartPayload(
    @SerialName("episode_id") val episodeId: String,
    val op: String = "download",
)

/**
 * Remove a previously-**completed** download from disk and clear the kernel's
 * `local_path` mapping. Verified against `PodcastAction::DeleteDownload` →
 * `handle_delete_download`, which only `clear_local_path`s + `remove_file`s.
 *
 * NOTE: this does **not** cancel an in-flight download — it operates on the
 * store's `local_path`, which is only stamped on completion. To cancel an
 * active/queued/paused download use [CancelDownloadPayload] on the
 * `podcast.player` namespace instead.
 */
@Serializable
data class DownloadDeletePayload(
    @SerialName("episode_id") val episodeId: String,
    val op: String = "delete_download",
)

// ── `podcast.player` namespace payloads ───────────────────────────────────

/**
 * Cancel an active, paused, or queued download. Verified against
 * `PlayerAction::CancelDownload` → `q.cancel(&episode_id)`. This is the
 * correct op for the "cancel" affordance on an *active* download row —
 * `delete_download` (podcast namespace) only removes a finished file.
 */
@Serializable
data class CancelDownloadPayload(
    @SerialName("episode_id") val episodeId: String,
    val op: String = "cancel_download",
)

@Serializable
data class PlayPayload(
    @SerialName("episode_id") val episodeId: String,
    val op: String = "play",
)

@Serializable
data class PausePayload(val op: String = "pause")

@Serializable
data class SeekPayload(
    @SerialName("position_secs") val positionSecs: Double,
    val op: String = "seek",
)

@Serializable
data class SetSpeedPayload(
    val speed: Float,
    val op: String = "set_speed",
)

/**
 * Arm (`secs = N`) or clear (`secs = null`) the sleep timer. Field is `secs`
 * (not `seconds`) per `PlayerAction::SetSleepTimer { secs: Option<u64> }`.
 */
@Serializable
data class SleepTimerPayload(
    val secs: Int?,
    val op: String = "set_sleep_timer",
)

@Serializable
data class PlayNextPayload(val op: String = "play_next")

// ── `podcast.queue` namespace payloads ────────────────────────────────────

@Serializable
data class QueueAddNextPayload(
    @SerialName("episode_id") val episodeId: String,
    val op: String = "add_next",
)

@Serializable
data class QueueAddLastPayload(
    @SerialName("episode_id") val episodeId: String,
    val op: String = "add_last",
)

@Serializable
data class QueueRemovePayload(
    @SerialName("episode_id") val episodeId: String,
    val op: String = "remove",
)

@Serializable
data class QueueClearPayload(val op: String = "clear")

// ── `podcast.settings` namespace payloads ─────────────────────────────────

/**
 * Set the default playback rate. Verified against
 * `SettingsAction::SetDefaultPlaybackRate { rate: f64 }` — the field is
 * `rate` (NOT the task-spec's `set_default_speed`/`rate` on `podcast`).
 * The kernel clamps server-side to `[0.5, 3.0]`.
 */
@Serializable
data class SetDefaultPlaybackRatePayload(
    val rate: Double,
    val op: String = "set_default_playback_rate",
)

/**
 * Toggle delete-downloaded-file-after-played. Verified against
 * `SettingsAction::SetAutoDeleteDownloadsAfterPlayed { enabled: bool }`.
 */
@Serializable
data class SetAutoDeleteDownloadsPayload(
    val enabled: Boolean,
    val op: String = "set_auto_delete_downloads_after_played",
)

/**
 * Thin wrapper around `KernelBridge.dispatchAction`. Encodes a typed,
 * op-tagged payload to JSON (one place, one config) and forwards it to the
 * given namespace. No business logic, no state (D5/D8) — the caller picks the
 * namespace + payload; the kernel decides the outcome and reports it on the
 * next snapshot.
 */
object PodcastActionDispatcher {
    @PublishedApi
    internal val json: Json = Json { encodeDefaults = true; ignoreUnknownKeys = true }

    /** Dispatch a typed, op-tagged payload to [namespace]. Returns the envelope JSON. */
    inline fun <reified T> dispatch(
        bridge: KernelBridge,
        namespace: String,
        payload: T,
    ): String? = bridge.dispatchAction(namespace, json.encodeToString<T>(payload))
}
