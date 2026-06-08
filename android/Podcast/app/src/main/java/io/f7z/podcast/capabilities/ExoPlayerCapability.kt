package io.f7z.podcast.capabilities

import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.os.Handler
import android.os.Looper
import androidx.media3.common.MediaItem
import androidx.media3.common.Player
import io.f7z.podcast.KernelBridge
import io.f7z.podcast.service.PlaybackServiceBinder
import io.f7z.podcast.service.PodcastPlaybackService
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

/**
 * `nmp.audio.capability` executor for Android — the ExoPlayer-backed
 * counterpart to `ios/Podcast/Podcast/Capabilities/AudioCapability.swift`.
 *
 * **Replaces `AudioCapabilityStub`.** The capability surface is the same
 * single `handleCommand(commandJson: String)` entry point M13.A landed,
 * so the (eventually) kernel-side router that lands in a follow-up PR
 * binds against an unchanged seam.
 *
 * **Execution model:**
 *
 *  * On `attach`, start `PodcastPlaybackService` (foreground media
 *    session) and bind a `Player.Listener` for playback-state reports.
 *  * On every `handleCommand`, translate the `AudioCommand` JSON envelope
 *    into ExoPlayer calls against the service-owned player.
 *  * Listener callbacks emit `AudioReport` JSON back to the kernel via
 *    `KernelBridge.capabilityReport`; any follow-up `AudioCommand` JSON is
 *    executed immediately, matching the iOS report channel.
 *
 * **Doctrine:**
 *
 *  * D5/D8 — pure executor. The only retained state is `lastReportedUrl`
 *    (so the listener can attribute reports to the current item — ExoPlayer
 *    exposes the URI only after `MediaItem` resolution) and the armed
 *    sleep-timer callback (ExoPlayer has no sleep-timer primitive, so the
 *    executor holds the wall-clock; the kernel still owns the policy per D9).
 *  * D6 — every entry point degrades silently. Malformed JSON, missing
 *    player, audio-focus refusal — all surface as no-ops; the kernel
 *    notices via the missing report.
 *  * D7 — reports, never decides. End-of-track does NOT auto-advance;
 *    the listener emits `Stopped` and the kernel's `PlayerActor` picks
 *    the next move.
 *  * D8 — position reports are throttled to ≤4 Hz by the listener (see
 *    `ExoPlayerCapability+Listener.kt`); the kernel collapses bursts into
 *    the next snapshot tick.
 *
 * **Wire format:** identical to iOS (controlled by
 * `apps/nmp-app-podcast/src/capability/audio.rs`). The wire vocabulary
 * (`load` / `play` / `pause` / `seek` / `set_volume` / `set_speed` /
 * `set_sleep_timer` / `stop`) is parsed manually with `JsonObject` —
 * deliberately not a `sealed class` hierarchy so unknown variants drop
 * silently per D6 instead of throwing a `SerializationException`.
 */
class ExoPlayerCapability(
    private val bridge: KernelBridge,
    private val context: Context,
) {
    @PublishedApi
    internal val json: Json = Json {
        ignoreUnknownKeys = true
        encodeDefaults = true
    }

    /**
     * The `Player.Listener` that emits `AudioReport` envelopes. Held so
     * `detach()` can deregister it without removing every other listener
     * the service may attach later (notifications, debug logging).
     */
    private var listener: Player.Listener? = null

    /**
     * The last URL the kernel asked us to load. ExoPlayer's `mediaMetadata`
     * doesn't preserve the source URI in a stable accessor we can rely on,
     * so we cache the one the kernel sent and echo it back on every
     * report. Cleared on `Stop`.
     */
    @Volatile
    private var lastReportedUrl: String = ""

    /**
     * Sleep-timer plumbing. ExoPlayer has no sleep-timer primitive (vs.
     * iOS's `DispatchSourceTimer`), so we hold the wall-clock ourselves with
     * a main-looper `Handler` — the same looper `ExoPlayerReportListener`
     * ticks on, which keeps the fired `stop` re-dispatch on the thread the
     * player commands already assume.
     *
     * D9 — the kernel owns sleep-timer *policy*. We only hold the clock: on
     * expiry we emit a `SleepTimerFired` report and let `crate::player`
     * decide whether to stop, fade, or extend. We never pause/stop the
     * player ourselves on expiry. The kernel's reply (`{"type":"stop"}`) is
     * re-dispatched through `emit`'s follow-up channel, matching the iOS
     * round-trip.
     */
    private val sleepTimerHandler = Handler(Looper.getMainLooper())

    /**
     * The currently-armed sleep-timer callback, or `null` when no timer is
     * running. Held so `cancelSleepTimer` can remove the exact pending
     * callback (and so re-arming replaces rather than stacks).
     */
    private var sleepTimerRunnable: Runnable? = null

    // ─────────────────────────────────────────────────────────────────────
    // Lifecycle
    // ─────────────────────────────────────────────────────────────────────

    /**
     * Idempotent. Starts the playback service (so the OS keeps the process
     * alive while audio plays) and attaches a listener to the service's
     * `ExoPlayer` once it publishes.
     *
     * Safe to call on every app foreground. The service's `onCreate` is
     * the one that builds the player; this method is the binder side.
     */
    fun attach() {
        ensureServiceStarted()
        bindListenerIfReady()
    }

    /**
     * Idempotent. Removes the listener but does NOT stop the service —
     * playback continues in the background per the foreground-service
     * contract. The service self-destructs on `onTaskRemoved` if paused.
     */
    fun detach() {
        cancelSleepTimer()
        val handle = PlaybackServiceBinder.current() ?: return
        listener?.let { handle.player.removeListener(it) }
        listener = null
    }

    /**
     * Process one capability command. The envelope is the `AudioCommand`
     * `serde`-tagged form: `{"type":"play"}`,
     * `{"type":"load","url":"…","position_secs":…}`, etc. See
     * `crate::capability::audio::AudioCommand`.
     *
     * Returns `Unit`. Failures (parse errors, missing player) drop
     * silently per D6.
     */
    fun handleCommand(commandJson: String) {
        val envelope = runCatching { json.parseToJsonElement(commandJson).jsonObject }
            .getOrNull() ?: return
        val type = envelope["type"]?.jsonPrimitive?.content ?: return

        // The service may not have published yet on the very first command —
        // attach() schedules a startService but onCreate runs asynchronously.
        // Re-attempt the bind before each command so the first `load` after
        // a cold start finds the player.
        bindListenerIfReady()
        val handle = PlaybackServiceBinder.current() ?: return
        val player = handle.player

        when (type) {
            "load" -> {
                val url = envelope["url"]?.jsonPrimitive?.content.orEmpty()
                val position = envelope["position_secs"]?.jsonPrimitive?.content?.toDoubleOrNull() ?: 0.0
                if (url.isBlank()) {
                    emit(buildFailedReport(url = "", error = "empty-url"))
                    return
                }
                lastReportedUrl = url
                // `setMediaItem(item, startPositionMs)` is the canonical
                // media3 1.4 API for resume-at-position — no separate
                // `seekTo` after `prepare` (which would either no-op
                // before the renderer is ready or trigger an extra seek
                // round-trip once it is).
                val startMs = (position * 1000.0).toLong().coerceAtLeast(0L)
                player.setMediaItem(MediaItem.fromUri(url), startMs)
                player.prepare()
            }
            "play" -> {
                // `playWhenReady = true` schedules playback as soon as the
                // renderer is ready. `play()` is sugar over the same op;
                // we use the explicit form so the intent is grep-able.
                player.playWhenReady = true
                player.play()
            }
            "pause" -> {
                player.pause()
            }
            "seek" -> {
                val pos = envelope["position_secs"]?.jsonPrimitive?.content?.toDoubleOrNull() ?: return
                player.seekTo((pos * 1000.0).toLong().coerceAtLeast(0L))
            }
            "set_volume" -> {
                val vol = envelope["volume"]?.jsonPrimitive?.content?.toFloatOrNull() ?: return
                player.volume = vol.coerceIn(0f, 1f)
            }
            "set_speed" -> {
                val speed = envelope["speed"]?.jsonPrimitive?.content?.toFloatOrNull() ?: return
                player.setPlaybackSpeed(speed.coerceIn(0.5f, 2.0f))
            }
            "set_sleep_timer" -> {
                // `{"type":"set_sleep_timer","secs":1800}` arms; `secs:null`
                // or a missing/zero `secs` cancels. Mirrors the iOS executor
                // `AudioCapability.armSleepTimer`. The kernel decides what to
                // do on expiry (D9); we only hold the wall-clock.
                val secs = envelope["secs"]?.jsonPrimitive?.content?.toLongOrNull()
                armSleepTimer(secs)
            }
            "stop" -> {
                // A `stop` ends the current item; the sleep timer is no
                // longer meaningful, so cancel it (mirrors iOS
                // `playerStop -> cancelSleepTimer`).
                cancelSleepTimer()
                player.stop()
                player.clearMediaItems()
                val previousUrl = lastReportedUrl
                lastReportedUrl = ""
                emit(buildStoppedReport(previousUrl))
            }
            else -> {
                // Unknown variant — drop. D6.
            }
        }
    }

    // ─────────────────────────────────────────────────────────────────────
    // Service plumbing
    // ─────────────────────────────────────────────────────────────────────

    private fun ensureServiceStarted() {
        // `startForegroundService` is the API 26+ entry point for kicking a
        // foreground service when the app isn't already foregrounded. The
        // service itself promotes via `startForeground` on first
        // `onPlayerCommand` — media3 manages that promotion automatically.
        val intent = Intent(context, PodcastPlaybackService::class.java)
        intent.component = ComponentName(context, PodcastPlaybackService::class.java)
        // `startService` is sufficient while the app is foregrounded; the
        // OS upgrades the binding once `MediaSessionService` requests the
        // foreground state via its built-in notification.
        runCatching { context.startService(intent) }
    }

    private fun bindListenerIfReady() {
        val handle = PlaybackServiceBinder.current() ?: return
        if (listener != null) return
        val newListener = ExoPlayerReportListener(
            emit = ::emit,
            currentUrl = { lastReportedUrl },
            playerProvider = { PlaybackServiceBinder.current()?.player },
        )
        handle.player.addListener(newListener)
        listener = newListener
    }

    // ─────────────────────────────────────────────────────────────────────
    // Sleep timer (D9: Android holds the wall-clock; the kernel decides)
    // ─────────────────────────────────────────────────────────────────────

    /**
     * Arm a wall-clock sleep timer that fires after [secs] seconds, or
     * cancel any running timer when [secs] is `null`/`0`. Idempotent
     * re-arm: an existing timer is cancelled first so timers never stack.
     */
    private fun armSleepTimer(secs: Long?) {
        cancelSleepTimer()
        if (secs == null || secs <= 0L) return
        val runnable = Runnable { onSleepTimerFire() }
        sleepTimerRunnable = runnable
        sleepTimerHandler.postDelayed(runnable, secs * 1000L)
    }

    /** Cancel the armed sleep timer, if any. Safe to call when none is set. */
    private fun cancelSleepTimer() {
        sleepTimerRunnable?.let { sleepTimerHandler.removeCallbacks(it) }
        sleepTimerRunnable = null
    }

    /**
     * Sleep-timer expiry. D7/D9: we do NOT pause/stop the player here — we
     * only report. The kernel's `PlayerActor` replies with the actual
     * `{"type":"stop"}` command, which `emit`'s follow-up channel
     * re-dispatches through `handleCommand`. Clear the runnable ref BEFORE
     * emitting so a re-arm triggered during that re-dispatch isn't clobbered.
     */
    private fun onSleepTimerFire() {
        sleepTimerRunnable = null
        emit(buildSleepTimerFiredReport())
    }

    // ─────────────────────────────────────────────────────────────────────
    // Report wire
    // ─────────────────────────────────────────────────────────────────────

    /**
     * Emit an `AudioReport` envelope to the kernel via the JNI report
     * channel. Visibility is package-private so the listener file can
     * call it without going through reflection.
     */
    internal fun emit(report: JsonObject) {
        val payload = json.encodeToString(JsonObject.serializer(), report)
        bridge.capabilityReport(NAMESPACE, payload)?.let { followUp ->
            handleCommand(followUp)
        }
    }

    private fun buildStoppedReport(url: String): JsonObject = buildJsonObject {
        put("type", JsonPrimitive("stopped"))
        if (url.isNotBlank()) put("url", JsonPrimitive(url))
    }

    /**
     * `AudioReport::SleepTimerFired` — payloadless, tagged `sleep_timer_fired`.
     * Matches `apps/nmp-app-podcast/src/capability/audio.rs::AudioReport`.
     */
    private fun buildSleepTimerFiredReport(): JsonObject = buildJsonObject {
        put("type", JsonPrimitive("sleep_timer_fired"))
    }

    private fun buildFailedReport(url: String, error: String): JsonObject = buildJsonObject {
        put("type", JsonPrimitive("failed"))
        put("url", JsonPrimitive(url))
        put("error", JsonPrimitive(error))
    }

    companion object {
        /**
         * Matches `apps/nmp-app-podcast/src/capability/audio.rs::AUDIO_CAPABILITY_NAMESPACE`.
         * If the Rust constant moves, this must move with it.
         */
        const val NAMESPACE: String = "nmp.audio.capability"
    }
}
