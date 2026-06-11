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
import io.f7z.podcast.capabilities.AndroidCapabilityRouter
import io.f7z.podcast.capabilities.DownloadCapability
import io.f7z.podcast.capabilities.ExoPlayerCapability
import io.f7z.podcast.capabilities.HttpCapability
import io.f7z.podcast.security.KeystoreManager
import io.f7z.podcast.ui.AppNavigation
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext
import kotlin.coroutines.coroutineContext

/**
 * Single-Activity Compose host for the M13 Android surface.
 *
 * The activity itself is a thin owner of the [`KernelBridge`] lifecycle:
 * it starts the Rust actor on composition, consumes pushed snapshot frames
 * as the kernel emits them, and hands the decoded [`PodcastSnapshot`] down
 * to [`AppNavigation`].
 *
 * D5/D8 — no state is held here beyond the snapshot mirror; every screen
 * derives what it shows from the latest snapshot tick. Action dispatches
 * go through `bridge.dispatchAction(...)` in each screen, never through
 * the activity.
 *
 * Audio capability (PR 13):
 *  - `ExoPlayerCapability` replaces the M13.A `AudioCapabilityStub`. The
 *    capability holds a `Player.Listener` against the `ExoPlayer` owned
 *    by `PodcastPlaybackService`, emits `AudioReport` envelopes through
 *    `KernelBridge.capabilityReport`, and consumes kernel-issued
 *    `AudioCommand` envelopes through the registered NMP capability router.
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
 * Root composable — owns the [`KernelBridge`] lifecycle and the reactive
 * snapshot loop, then delegates rendering to [`AppNavigation`].
 *
 * Snapshot delivery is push-driven (issue #320): the loop blocks on
 * `bridge.nextUpdate()`, which drains the kernel's update channel
 * (`apps/nmp-app-podcast/src/android.rs::on_update` → `nativeNextUpdate`'s
 * blocking `recv`). It wakes the moment Rust emits a new frame and otherwise
 * costs nothing — no fixed timer, no repeated `podcastSnapshot()` pulls. This
 * matches the iOS push path and the NMP rule that there is no polling at any
 * layer. A single initial `podcastSnapshot()` pull paints the first frame so
 * the UI isn't blank until the kernel's first emit.
 */
@Composable
private fun PodcastRoot() {
    val context = LocalContext.current
    val bridge = remember { KernelBridge() }
    // `attach` starts `PodcastPlaybackService` so the OS keeps the process
    // alive while audio plays, then binds a `Player.Listener` to the
    // service-owned player. NMP commands arrive through AndroidCapabilityRouter.
    val audio = remember(bridge) {
        ExoPlayerCapability(bridge = bridge, context = context.applicationContext)
    }
    val download = remember(bridge) {
        DownloadCapability(bridge = bridge, context = context.applicationContext)
    }
    val http = remember(bridge) { HttpCapability(bridge = bridge) }
    val router = remember(audio, http) {
        AndroidCapabilityRouter(audio = audio, http = http)
    }
    var snapshot by remember { mutableStateOf<PodcastSnapshot?>(null) }

    DisposableEffect(bridge, audio, download, http, router) {
        // Bind the kernel's persistence directory FIRST — before any write
        // (identity import below) and before `start` — so the kernel reloads
        // `podcasts.json`, `identity.json`, the Up-Next queue, per-podcast keys,
        // relay config, and the triage cache from disk, and so the subsequent
        // identity write actually persists. Mirrors the iOS register →
        // set_data_dir → start ordering. Without this the kernel keeps all
        // state in memory only and loses it on every process restart.
        bridge.setDataDir(context.filesDir.absolutePath)

        // Restore a previously-imported identity before the actor starts so the
        // first snapshot already reflects the signed-in state. Dispatches the
        // canonical `podcast.identity` ImportNsec (the bridge constructor's
        // `nativeNew` has already run via `remember { KernelBridge() }`); the
        // legacy `signinNsec` stub is intentionally NOT used — it feeds a store
        // the Identity screen never reads.
        KeystoreManager.loadNsec(context)?.let { stored ->
            IdentityActions.importNsec(bridge, stored)
        }
        ProviderCredentialActions.reloadProviderApiKeys(context, bridge)
        ProviderCredentialActions.syncSttKeysPresent(context, bridge)
        bridge.registerCapabilityRouter(router)
        http.start()
        audio.attach()
        bridge.start()
        onDispose {
            download.detach()
            bridge.stop()
            audio.detach()
            http.stop()
            bridge.unregisterCapabilityRouter()
            bridge.free()
        }
    }

    LaunchedEffect(bridge) {
        // First paint: one-shot pull off the projection cache so the UI renders
        // immediately instead of waiting for the kernel's first push frame.
        withContext(Dispatchers.IO) { bridge.podcastSnapshot() }
            ?.let { raw ->
                SnapshotCodec.decode(raw)?.let { first ->
                    snapshot = first
                    download.reconcile(first.downloads?.active)
                }
            }

        // Steady state: block on the kernel's push channel. `nextUpdate()` parks
        // on the Rust-side `recv` (≤250 ms bounded so cancellation is prompt) and
        // returns the moment a new frame arrives — reactive, not timed. A `null`
        // return means "no new frame yet"; we re-park without touching state.
        while (true) {
            coroutineContext.ensureActive()
            val raw = withContext(Dispatchers.IO) { bridge.nextUpdate() } ?: continue
            val next = SnapshotCodec.decodeEnvelope(raw) ?: continue
            snapshot = next
            download.reconcile(next.downloads?.active)
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
