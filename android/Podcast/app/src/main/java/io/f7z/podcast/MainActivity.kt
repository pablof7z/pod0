package io.f7z.podcast

import android.os.Bundle
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import io.f7z.podcast.capabilities.ExoPlayerCapability
import io.f7z.podcast.security.KeystoreManager
import io.f7z.podcast.ui.AppNavigation
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext

/**
 * Single-Activity Compose host for the M13 Android surface.
 *
 * The activity itself is a thin owner of the [`KernelBridge`] lifecycle:
 * it starts the Rust actor on composition, polls the snapshot at ~2 Hz,
 * and hands the decoded [`PodcastSnapshot`] down to [`AppNavigation`].
 *
 * D5/D8 — no state is held here beyond the snapshot mirror; every screen
 * derives what it shows from the latest snapshot tick. Action dispatches
 * go through `bridge.dispatchAction(...)` in each screen, never through
 * the activity.
 *
 * Audio capability (PR 13):
 *  - `ExoPlayerCapability` replaces the M13.A `AudioCapabilityStub`. The
 *    capability holds a `Player.Listener` against the `ExoPlayer` owned
 *    by `PodcastPlaybackService`, emits `AudioReport` envelopes via
 *    `KernelBridge.nmpCapabilityReport`, and consumes `AudioCommand`
 *    envelopes through `handleCommand` (called by the kernel's
 *    capability router when that wiring lands; until then the kernel
 *    sees the executor via the report channel only).
 *  - Toast surface for `PodcastSnapshot.toast` via `LaunchedEffect`.
 */
class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            MaterialTheme {
                PodcastRoot()
            }
        }
    }
}

/**
 * Root composable — owns the [`KernelBridge`] lifecycle and the snapshot
 * polling loop, then delegates rendering to [`AppNavigation`].
 *
 * The polling cadence (~2 Hz) intentionally trails the kernel's 4 Hz emit
 * rate so the snapshot decode work doesn't dominate the main thread. A
 * later milestone will swap this for a push-side callback driven by
 * `KernelBridge.nextUpdate` on a worker coroutine.
 */
@Composable
private fun PodcastRoot() {
    val context = LocalContext.current
    val bridge = remember { KernelBridge() }
    // PR 13 — replace the M13.A stub with the real ExoPlayer executor.
    // `attach` starts `PodcastPlaybackService` so the OS keeps the process
    // alive while audio plays, then binds a `Player.Listener` to the
    // service-owned player. `detach` removes the listener on dispose; the
    // service self-terminates from its own `onTaskRemoved` hook when the
    // user swipes the app away while paused.
    val audio = remember(bridge) {
        ExoPlayerCapability(bridge = bridge, context = context.applicationContext)
    }
    var snapshot by remember { mutableStateOf<PodcastSnapshot?>(null) }

    DisposableEffect(bridge, audio) {
        // Restore a previously-imported identity before the actor starts so the
        // first snapshot already reflects the signed-in state. Dispatches the
        // canonical `podcast.identity` ImportNsec (the bridge constructor's
        // `nativeNew` has already run via `remember { KernelBridge() }`); the
        // legacy `signinNsec` stub is intentionally NOT used — it feeds a store
        // the Identity screen never reads.
        KeystoreManager.loadNsec(context)?.let { stored ->
            IdentityActions.importNsec(bridge, stored)
        }
        bridge.start()
        audio.attach()
        onDispose {
            audio.detach()
            bridge.stop()
            bridge.free()
        }
    }

    LaunchedEffect(bridge) {
        while (true) {
            val raw = withContext(Dispatchers.IO) { bridge.podcastSnapshot() }
            snapshot = SnapshotCodec.decode(raw)
            delay(SNAPSHOT_POLL_INTERVAL_MS)
        }
    }

    // Toast — drives off the snapshot's `toast` field. `LaunchedEffect`
    // re-fires whenever the message text changes so back-to-back toasts
    // with the same text still surface (the kernel sends a fresh value
    // even when the body matches). Reuses the `context` already captured
    // at the top of the composable for the capability constructor.
    val toastMessage = snapshot?.toast
    LaunchedEffect(toastMessage) {
        if (!toastMessage.isNullOrBlank()) {
            Toast.makeText(context, toastMessage, Toast.LENGTH_SHORT).show()
        }
    }

    AppNavigation(snapshot = snapshot, bridge = bridge)
}

private const val SNAPSHOT_POLL_INTERVAL_MS: Long = 500
