package io.f7z.podcast.ui

import io.f7z.podcast.KernelBridge
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

/**
 * Kernel action wire contract for the Android shell.
 *
 * **The wire model is `(namespace, op-tagged body)` — not flat action ids.**
 *
 * The Rust kernel registers podcast-domain `ActionModule` namespaces including:
 *
 *  * `"podcast"`        — subscribe, unsubscribe, refresh_all, search_itunes,
 *                         download, delete_download, star_episode, …
 *  * `"podcast.player"` — play, pause, resume, seek, set_speed,
 *                         set_sleep_timer, stop, …
 *  * `"podcast.queue"`  — add_next, add_last, remove, clear.
 *  * `"podcast.settings"` — playback preferences, provider/model metadata.
 *  * `"podcast.tasks"`  — create/delete/enable/disable/run scheduled tasks.
 *
 * Each action is encoded as `{"op":"<variant>", …fields}` and the Rust
 * `#[serde(tag = "op", rename_all = "snake_case")]` discriminator routes it.
 * `KernelBridge.dispatchAction(namespace, body)` forwards through generated
 * `PodcastApp.dispatchPodcastAction(namespace, actionJson)`, exactly like the
 * iOS `kernel.dispatch(namespace:body:)` reference in
 * `App/Sources/Bridge/AppStateStore+KernelActions.swift`.
 *
 * Source of truth verified against:
 *  * `apps/nmp-app-podcast/src/ffi/actions/podcast_module.rs` (PodcastAction)
 *  * `apps/nmp-app-podcast/src/ffi/actions/player_module.rs`  (PlayerAction)
 *  * `apps/nmp-app-podcast/src/ffi/uniffi_facade.rs` (dispatch_podcast_action)
 *
 * NOTE: an earlier demo passed the *dotted op path* (e.g. `"podcast.player.play"`)
 * as the namespace argument. That string is not a registered namespace, so
 * those dispatches never reached the kernel. This file fixes the contract;
 * payloads carry their `op` as a defaulted field so `encodeDefaults = true`
 * emits the discriminator.
 *
 * Payload data classes ([SubscribePayload], [PlayPayload], [QueueAddNextPayload],
 * etc.) and namespace constants ([PodcastNamespace]) live in [ActionPayloads].
 */

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
