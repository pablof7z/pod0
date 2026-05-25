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
import io.f7z.podcast.capabilities.AudioCapabilityStub
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
 * M13.A extensions:
 *  - `AudioCapabilityStub` is instantiated here and retained for M13.B routing.
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
    val bridge = remember { KernelBridge() }
    // M13.A — instantiate the audio capability stub so its `handleCommand`
    // receiver is in place for the M13.B kernel command channel. `remember`
    // retains it for the lifetime of the composition; no further wiring
    // exists yet (the kernel router lands in M13.B).
    @Suppress("UNUSED_VARIABLE")
    val audio = remember(bridge) { AudioCapabilityStub(bridge) }
    var snapshot by remember { mutableStateOf<PodcastSnapshot?>(null) }

    DisposableEffect(bridge) {
        bridge.start()
        onDispose {
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
    // even when the body matches).
    val context = LocalContext.current
    val toastMessage = snapshot?.toast
    LaunchedEffect(toastMessage) {
        if (!toastMessage.isNullOrBlank()) {
            Toast.makeText(context, toastMessage, Toast.LENGTH_SHORT).show()
        }
    }

    AppNavigation(snapshot = snapshot, bridge = bridge)
}

private const val SNAPSHOT_POLL_INTERVAL_MS: Long = 500
