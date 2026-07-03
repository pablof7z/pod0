package io.f7z.podcast.capabilities

import androidx.media3.common.ForwardingPlayer
import androidx.media3.common.Player
import io.f7z.podcast.KernelDispatcher
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

/**
 * Wraps an ExoPlayer to route media3 transport commands through the Rust
 * kernel's `podcast.player` action dispatcher before executing on the
 * underlying player.
 *
 * When `bridge` is set, `play()`, `pause()`, `seekTo()`, `seekForward()`,
 * and `seekBack()` dispatch to the kernel instead of calling the parent
 * (wrapped) player directly. This ensures all transport commands — whether
 * initiated in-app or from the lock-screen / Bluetooth / Android Auto —
 * route through Rust for policy enforcement.
 *
 * When `bridge == null` (pre-initialization or standalone mode), commands
 * fall through to the parent player.
 *
 * Doctrine:
 * - The kernel is the sole authority for playback policy.
 * - This wrapper is a pure dispatcher; the kernel routes
 *   `AudioCommand` responses back through `ExoPlayerCapability.handleCommand()`,
 *   which executes on the wrapped player.
 * - There is no re-entrance risk: external MediaSession events (lock-screen
 *   play/pause) → this wrapper → kernel dispatch → `handleCommand()` → wrapped
 *   player (never the wrapper again).
 */
internal class KernelForwardingPlayer(
    innerPlayer: Player,
) : ForwardingPlayer(innerPlayer) {

    /**
     * The kernel dispatcher for dispatching `podcast.player` actions. Set by
     * `ExoPlayerCapability.bindListenerIfReady()` when the capability is ready;
     * cleared by `detach()`. Volatile for lock-free reads on the main thread.
     *
     * Typed as [KernelDispatcher] (not [io.f7z.podcast.KernelBridge]) so that
     * tests can inject a double without loading the native Rust library.
     */
    @Volatile
    var bridge: KernelDispatcher? = null

    private val json: Json = Json {
        ignoreUnknownKeys = true
        encodeDefaults = true
    }

    // MARK: - Transport command overrides

    /**
     * Route `play()` through the kernel as `{"op":"resume"}`.
     *
     * If bridge is set, dispatch to `podcast.player` and return immediately
     * (do NOT call `super.play()`). The kernel will route an `AudioCommand::Play`
     * back through `ExoPlayerCapability`, which executes on the inner player.
     *
     * If bridge is null, fall back to the parent player.
     */
    override fun play() {
        val b = bridge
        if (b != null) {
            dispatchToKernel(b, buildResumePayload())
        } else {
            super.play()
        }
    }

    /**
     * Route `pause()` through the kernel as `{"op":"pause"}`.
     */
    override fun pause() {
        val b = bridge
        if (b != null) {
            dispatchToKernel(b, buildPausePayload())
        } else {
            super.pause()
        }
    }

    /**
     * Route `seekTo()` through the kernel as `{"op":"seek","position_secs":<ms/1000>}`.
     */
    override fun seekTo(mediaItemIndex: Int, positionMs: Long) {
        val b = bridge
        if (b != null) {
            val positionSecs = positionMs / 1000.0
            dispatchToKernel(b, buildSeekPayload(positionSecs))
        } else {
            super.seekTo(mediaItemIndex, positionMs)
        }
    }

    /**
     * Route `seekForward()` through the kernel as `{"op":"skip_forward"}`.
     *
     * Delegating to Rust respects user-configured skip intervals and lets the
     * kernel apply chapter-snap / smart-skip policy. Consecutive taps while
     * paused accumulate correctly kernel-side (the kernel tracks position
     * independently of ExoPlayer's stale [currentPosition]).
     */
    override fun seekForward() {
        val b = bridge
        if (b != null) {
            dispatchToKernel(b, buildPayload("skip_forward"))
        } else {
            super.seekForward()
        }
    }

    /**
     * Route `seekBack()` through the kernel as `{"op":"skip_backward"}`.
     *
     * Same doctrine as [seekForward]: Rust owns the interval and accumulation.
     */
    override fun seekBack() {
        val b = bridge
        if (b != null) {
            dispatchToKernel(b, buildPayload("skip_backward"))
        } else {
            super.seekBack()
        }
    }

    // MARK: - Payload builders

    private fun buildResumePayload(): String =
        buildPayload("resume")

    private fun buildPausePayload(): String =
        buildPayload("pause")

    private fun buildSeekPayload(positionSecs: Double): String =
        json.encodeToString(
            JsonObject.serializer(),
            JsonObject(
                mapOf(
                    "op" to JsonPrimitive("seek"),
                    "position_secs" to JsonPrimitive(positionSecs),
                ),
            ),
        )

    private fun buildPayload(op: String): String =
        json.encodeToString(
            JsonObject.serializer(),
            JsonObject(
                mapOf(
                    "op" to JsonPrimitive(op),
                ),
            ),
        )

    // MARK: - Dispatch

    private fun dispatchToKernel(bridge: KernelDispatcher, payload: String) {
        bridge.dispatchAction(NAMESPACE, payload)
    }

    companion object {
        private const val NAMESPACE = "podcast.player"
    }
}
