package io.f7z.podcast.service

import android.content.Intent
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.session.MediaSession
import androidx.media3.session.MediaSessionService
import io.f7z.podcast.capabilities.KernelForwardingPlayer

/**
 * Foreground media-session host for the Android audio capability.
 *
 * The service owns the singular [`ExoPlayer`] instance. Both the
 * capability executor (`capabilities/ExoPlayerCapability.kt`) and the
 * platform's media routing (lock screen, Bluetooth, Android Auto, Wear)
 * talk to the same player through this session — exactly the pattern
 * media3 documents for podcast/long-form audio.
 *
 * **Why a service at all?** Android kills processes whose only activity
 * is backgrounded; without a `MediaSessionService` declared with the
 * `mediaPlayback` foreground type, the OS reclaims the JVM ≈30s after
 * the user leaves the app, taking ExoPlayer with it. The service is the
 * Android analogue of iOS `AVAudioSession.setCategory(.playback)` —
 * "this process plays audio out loud, keep it alive."
 *
 * **Doctrine:**
 *
 *  * D5/D8 — the service owns the player but holds no business state. It
 *    never decides what to load, when to advance, or how to react to
 *    end-of-track. Every such decision lives in the Rust `PlayerActor`
 *    (see `apps/nmp-app-podcast/src/player/mod.rs`) and round-trips
 *    through `KernelBridge.capabilityReport`.
 *  * D6 — service lifecycle never throws. Bind failures degrade to "no
 *    player available", surfaced by the capability returning early.
 *  * D7 — the service reports; it never invents follow-up commands. The
 *    `Player.Listener` in `ExoPlayerCapability` is the report channel.
 *
 * **Lifecycle:**
 *
 *  * `onCreate` constructs the player + session. Idempotent w.r.t. the
 *    OS because the service is started at most once per process.
 *  * `onTaskRemoved` honors the media3 convention: if the user swipes
 *    the app away while paused, we stop the service; if playback is
 *    active, we keep it foregrounded.
 *  * `onDestroy` releases the player and the session in lock-step.
 */
class PodcastPlaybackService : MediaSessionService() {

    private var mediaSession: MediaSession? = null
    private var exoPlayer: ExoPlayer? = null
    private var forwardingPlayer: KernelForwardingPlayer? = null

    override fun onCreate() {
        super.onCreate()

        // ExoPlayer is the actual playback engine. `setAudioAttributes` with
        // `handleAudioFocus = true` opts into the OS audio-focus protocol —
        // pauses on calls / Maps directions, resumes on dismissal. Mirrors
        // the `AVAudioSession.setCategory(.playback)` + interruption-observer
        // wiring the iOS executor uses.
        val player = ExoPlayer.Builder(this)
            .setAudioAttributes(
                AudioAttributes.Builder()
                    .setContentType(C.AUDIO_CONTENT_TYPE_SPEECH)
                    .setUsage(C.USAGE_MEDIA)
                    .build(),
                /* handleAudioFocus = */ true,
            )
            // `setHandleAudioBecomingNoisy(true)` pauses when headphones are
            // unplugged — the Android-side equivalent of iOS's route-change
            // notification. Without this, audio keeps blasting through the
            // speakers the moment Bluetooth disconnects.
            .setHandleAudioBecomingNoisy(true)
            .build()

        exoPlayer = player

        // Wrap the raw ExoPlayer with KernelForwardingPlayer so lock-screen,
        // Bluetooth, and Android Auto transport commands route through the
        // Rust kernel before executing.
        val outerPlayer = KernelForwardingPlayer(player)
        forwardingPlayer = outerPlayer

        // `MediaSession` is the glue that exposes the forwarding player to
        // the platform's media controls (lock screen, Bluetooth, Auto).
        // Transport commands now route through the kernel-forwarding wrapper.
        mediaSession = MediaSession.Builder(this, outerPlayer).build()

        // Publish the binder to the in-process holder so the capability and
        // the activity can attach the `Player.Listener` and dispatch commands.
        PlaybackServiceBinder.publish(player, outerPlayer, mediaSession)
    }

    /**
     * Required by `MediaSessionService` — system clients (Bluetooth, lock
     * screen, etc.) discover the session through this hook. Returning the
     * single owned session is the canonical media3 pattern.
     */
    override fun onGetSession(controllerInfo: MediaSession.ControllerInfo): MediaSession? =
        mediaSession

    /**
     * If the user swipes the app away while paused, stop the service so the
     * process can be reclaimed. If audio is playing, keep the service
     * running — this is the media3-recommended behaviour for podcast apps.
     */
    override fun onTaskRemoved(rootIntent: Intent?) {
        val player = exoPlayer
        if (player == null || !player.playWhenReady || player.mediaItemCount == 0) {
            stopSelf()
        }
        super.onTaskRemoved(rootIntent)
    }

    override fun onDestroy() {
        PlaybackServiceBinder.clear()
        forwardingPlayer?.bridge = null
        mediaSession?.run {
            player.release()
            release()
        }
        mediaSession = null
        exoPlayer = null
        forwardingPlayer = null
        super.onDestroy()
    }
}
