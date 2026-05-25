package io.f7z.podcast.ui

import io.f7z.podcast.KernelBridge
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

/**
 * Action ids — string discriminators the Rust kernel matches on. Mirrors
 * the `pub const ACTION_*` definitions in
 * `apps/nmp-app-podcast/src/ffi/actions/mod.rs`. Kept colocated with the
 * dispatcher so a screen ships one import for the (id, payload) pair.
 *
 * iOS has the same constants regenerated from the Rust source of truth via
 * `nmp-codegen`; Kotlin gets the same once M14 lands. Until then, these are
 * hand-mirrored — each id is duplicated below to keep the wire contract
 * explicit and grep-able.
 */
object PodcastActionIds {
    const val PLAYER_PLAY = "podcast.player.play"
    const val PLAYER_PAUSE = "podcast.player.pause"
    const val PLAYER_SEEK = "podcast.player.seek"
    const val PLAYER_SET_SPEED = "podcast.player.set_speed"
    const val PLAYER_STOP = "podcast.player.stop"
}

/**
 * Typed action payloads matching the Rust `actions::*Action` structs.
 *
 * Only the M13.C-relevant subset is mirrored here (Player surface). Voice,
 * Briefing, and Agent payloads land in their own screens when M13.D wires
 * them up. Each struct is `@Serializable` so the Compose layer can encode
 * via `Json.encodeToString` without hand-rolling the shape.
 */
@Serializable
data class PlayActionPayload(@SerialName("episode_id") val episodeId: String)

@Serializable
data class SeekActionPayload(@SerialName("position_secs") val positionSecs: Double)

@Serializable
data class SetSpeedActionPayload(val speed: Float)

/**
 * Thin wrapper around `KernelBridge.dispatchAction` that
 *
 *  1. Encodes a typed payload to JSON (one place, one configuration).
 *  2. Returns the kernel's response envelope unchanged.
 *
 * No business logic, no caching, no state (D5/D8). The caller is responsible
 * for picking the right action id; this object only owns the encode + the
 * JNI hop.
 *
 * Why not `KernelBridge.nmpActionDispatch(json)` as the task spec suggests?
 * The bridge's actual public surface (`dispatchAction(namespace, payload)`)
 * already takes a separate namespace string + payload JSON, mirroring the
 * Swift `dispatchAction(namespace:body:)` contract. Re-routing through a
 * single-string method would be a regression — we'd lose the namespace/payload
 * separation Rust uses to demux. M13.A may collapse this; until they do, the
 * Compose layer talks to the existing two-arg method.
 */
object PodcastActionDispatcher {
    @PublishedApi
    internal val json: Json = Json { encodeDefaults = true; ignoreUnknownKeys = true }

    /** Dispatch with a typed payload. Returns the raw envelope JSON. */
    inline fun <reified T> dispatch(
        bridge: KernelBridge,
        actionId: String,
        payload: T,
    ): String? {
        val body = json.encodeToString<T>(payload)
        return bridge.dispatchAction(actionId, body)
    }

    /** Dispatch with no payload (pause / stop / cancel_all). */
    fun dispatchEmpty(bridge: KernelBridge, actionId: String): String? =
        bridge.dispatchAction(actionId, "{}")
}
