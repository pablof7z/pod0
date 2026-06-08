package io.f7z.podcast.capabilities

import android.os.Handler
import android.os.Looper
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject

/**
 * `Player.Listener` that translates ExoPlayer state transitions into
 * `AudioReport` JSON envelopes (`playing` / `paused` / `stopped` /
 * `failed`) and emits them through [`ExoPlayerCapability.emit`].
 *
 * **Split out of `ExoPlayerCapability.kt`** to stay under the 300-line
 * soft limit (`AGENTS.md`). The capability owns the player lifecycle and
 * the command-side; this listener owns the report-side.
 *
 * **Position reporting — event-driven first, sampled only where forced:**
 *
 * Significant position events are reported event-driven, with no timer:
 * `onIsPlayingChanged` (play/pause, incl. audio-focus interruptions),
 * `onPlaybackStateChanged(STATE_ENDED)` (end-of-track → `Stopped`), and
 * `onPlayerError` (→ `Failed`). Each reads `getCurrentPosition()` at the
 * instant of the event, so seeks/starts/stops carry an exact position
 * without any periodic sampling.
 *
 * **Within-segment progress is the one sampled signal (documented platform
 * exception, `docs/BACKLOG.md` `android-exoplayer-position-sampling`):**
 *
 * ExoPlayer fires no per-second position callback — `getCurrentPosition` is
 * poll-only — so smooth progress between events has no event source on the
 * platform. We schedule a Handler tick at `POSITION_TICK_MS` ONLY while
 * playback is active; it stops the instant `isPlayingChanged(false)` fires, so
 * there are no idle wakeups. The interval is a conservative 1 Hz (mirroring
 * the iOS executor's cadence); the kernel collapses anything faster into its
 * next snapshot tick anyway (D8), so a faster rate would only burn wakeups.
 *
 * **Doctrine:**
 *
 *  * D5 — no state beyond the throttle bookkeeping (`ticking`).
 *  * D7 — reports observations only. End-of-track emits `Stopped`; the
 *    kernel decides whether to advance.
 */
internal class ExoPlayerReportListener(
    private val emit: (JsonObject) -> Unit,
    private val currentUrl: () -> String,
    private val playerProvider: () -> Player?,
) : Player.Listener {

    private val handler = Handler(Looper.getMainLooper())
    private var ticking = false

    /**
     * Fires when ExoPlayer transitions between `STATE_IDLE`, `STATE_BUFFERING`,
     * `STATE_READY`, and `STATE_ENDED`. Used for the terminal `ENDED` →
     * `Stopped` translation; the actual play/pause distinction is driven
     * by `onIsPlayingChanged` which fires more reliably and respects
     * audio-focus interruptions.
     */
    override fun onPlaybackStateChanged(playbackState: Int) {
        when (playbackState) {
            Player.STATE_ENDED -> {
                stopTicking()
                emit(buildJsonObject {
                    put("type", JsonPrimitive("stopped"))
                    val url = currentUrl()
                    if (url.isNotBlank()) put("url", JsonPrimitive(url))
                })
            }
            // STATE_READY + STATE_BUFFERING + STATE_IDLE — `isPlayingChanged`
            // is the canonical signal for play/pause reports; we don't fire
            // from here to avoid duplicate envelopes.
            else -> Unit
        }
    }

    /**
     * The canonical play/pause signal — fires whenever the actual playback
     * state flips (which can lag `playWhenReady` while buffering). Mirrors
     * the iOS executor's `timeControlStatus` KVO observation.
     */
    override fun onIsPlayingChanged(isPlaying: Boolean) {
        if (isPlaying) {
            emit(buildPlayingReport())
            startTicking()
        } else {
            stopTicking()
            // Don't emit a Paused report for a STATE_ENDED transition —
            // `onPlaybackStateChanged(STATE_ENDED)` will have already
            // emitted Stopped, and a follow-up Paused would confuse the
            // kernel's PlayerActor projection.
            val player = playerProvider() ?: return
            if (player.playbackState == Player.STATE_ENDED) return
            emit(buildPausedReport())
        }
    }

    /**
     * Event-driven exact-position signal for seeks (and other discontinuities
     * such as auto-transitions). ExoPlayer fires this the moment the playhead
     * jumps, so we report the new position immediately instead of waiting up to
     * one sampling interval for the next tick. Only meaningful while playing —
     * a paused seek is already covered by the kernel's own seek echo, and a
     * `Playing` report while paused would misrepresent state, so we gate on it.
     */
    override fun onPositionDiscontinuity(
        oldPosition: Player.PositionInfo,
        newPosition: Player.PositionInfo,
        reason: Int,
    ) {
        val player = playerProvider() ?: return
        if (player.isPlaying) emit(buildPlayingReport())
    }

    /**
     * Fires when `setPlaybackSpeed` updates the rate. We don't echo a
     * dedicated report for this (the iOS executor doesn't either — speed
     * is a configuration, not an observation), but the override is here
     * to suppress the default no-op log noise media3 emits on rate
     * changes.
     */
    @Suppress("EmptyFunctionBlock")
    override fun onPlaybackParametersChanged(playbackParameters: androidx.media3.common.PlaybackParameters) {}

    /**
     * Surface decoder errors, network errors, and DRM errors as
     * `AudioReport::Failed`. ExoPlayer attaches the URI, error code, and
     * cause chain to the exception; we hand the user-readable message
     * back so the kernel can log it.
     */
    override fun onPlayerError(error: PlaybackException) {
        stopTicking()
        emit(buildJsonObject {
            put("type", JsonPrimitive("failed"))
            put("url", JsonPrimitive(currentUrl()))
            put("error", JsonPrimitive(error.errorCodeName))
        })
    }

    // ─── Position ticks ──────────────────────────────────────────────────

    private fun startTicking() {
        if (ticking) return
        ticking = true
        handler.postDelayed(tickRunnable, POSITION_TICK_MS)
    }

    private fun stopTicking() {
        ticking = false
        handler.removeCallbacks(tickRunnable)
    }

    private val tickRunnable: Runnable = object : Runnable {
        override fun run() {
            if (!ticking) return
            val player = playerProvider()
            if (player == null || !player.isPlaying) {
                ticking = false
                return
            }
            emit(buildPlayingReport())
            handler.postDelayed(this, POSITION_TICK_MS)
        }
    }

    // ─── Report builders ─────────────────────────────────────────────────

    private fun buildPlayingReport(): JsonObject {
        val player = playerProvider()
        val positionMs = player?.currentPosition ?: 0L
        val durationMs = player?.duration ?: 0L
        return buildJsonObject {
            put("type", JsonPrimitive("playing"))
            put("url", JsonPrimitive(currentUrl()))
            put("position_secs", JsonPrimitive(positionMs.coerceAtLeast(0L) / 1000.0))
            // ExoPlayer returns `C.TIME_UNSET` (Long.MIN_VALUE) before the
            // item is resolved; clamp to 0 so the wire is well-formed.
            put("duration_secs", JsonPrimitive(if (durationMs > 0) durationMs / 1000.0 else 0.0))
        }
    }

    private fun buildPausedReport(): JsonObject {
        val player = playerProvider()
        val positionMs = player?.currentPosition ?: 0L
        return buildJsonObject {
            put("type", JsonPrimitive("paused"))
            put("url", JsonPrimitive(currentUrl()))
            put("position_secs", JsonPrimitive(positionMs.coerceAtLeast(0L) / 1000.0))
        }
    }

    companion object {
        /**
         * Within-segment progress sampling cadence — the documented platform
         * exception (see the class doc and `docs/BACKLOG.md`
         * `android-exoplayer-position-sampling`). Conservative 1 Hz, mirroring
         * the iOS executor and well under the ≤4 Hz ceiling the canonical
         * `AudioReport::Playing` doc allows
         * (`apps/nmp-app-podcast/src/capability/audio.rs`). The kernel collapses
         * anything faster into its next snapshot tick (D8, default
         * `emit_hz = 4`), so a higher rate would only spend wakeups for no
         * additional fidelity. Significant position events (seek/play/pause/
         * end/error) are reported event-driven and do not depend on this tick.
         */
        private const val POSITION_TICK_MS: Long = 1000
    }
}
