package io.f7z.podcast

import android.os.Bundle
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
import io.f7z.podcast.ui.AppNavigation
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext

/**
 * Single-Activity Compose host for the M13.C+D Android surface.
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
 * The M2.F single-screen `PodcastRoot` has been replaced by
 * [`AppNavigation`] which composes the four-tab bottom-bar surface; the
 * M2.F "Sign in (stub)" CTA + status card are subsumed by the new
 * Identity + Home screens respectively.
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

    AppNavigation(snapshot = snapshot, bridge = bridge)
}

private const val SNAPSHOT_POLL_INTERVAL_MS: Long = 500
