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

@Serializable
data class DownloadDeletePayload(
    @SerialName("episode_id") val episodeId: String,
    val op: String = "delete_download",
)

// ── `podcast.player` namespace payloads ───────────────────────────────────

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
