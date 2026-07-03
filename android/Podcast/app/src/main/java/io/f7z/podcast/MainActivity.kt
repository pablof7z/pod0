package io.f7z.podcast

import android.os.Bundle
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import io.f7z.podcast.capabilities.AndroidCapabilityRouter
import io.f7z.podcast.capabilities.DownloadCapability
import io.f7z.podcast.capabilities.ExoPlayerCapability
import io.f7z.podcast.capabilities.HttpCapability
import io.f7z.podcast.security.KeystoreManager
import io.f7z.podcast.signing.ExternalSignerCapabilityBridge
import io.f7z.podcast.ui.AppNavigation
import io.f7z.podcast.ui.FeedbackSheet
import io.f7z.podcast.ui.ShakeFeedbackDetector
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext
import kotlin.coroutines.coroutineContext

/**
 * Amber's Android package — the `nostrsigner:` signer this app targets for
 * NIP-55 sign-in (ADR-0048). Mirrors `KNOWN_NOSTR_SIGNERS` in
 * `signing/ExternalSignerWire.kt`; the package also appears in `<queries>` in
 * AndroidManifest.xml so `PackageManager` detection works on API 30+.
 */
private const val AMBER_SIGNER_PACKAGE = "com.greenart7c3.nostrsigner"

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

    /**
     * ADR-0048 Stage 2 — the D7 host adapter for the `external_signer`
     * capability (NIP-55 Amber). Owns the Activity Result launcher, which MUST
     * be registered in `onCreate` before first `onStart`; raw Amber results are
     * routed back to the Rust driver through the kernel sink installed by
     * [PodcastRoot] once its [KernelBridge] exists.
     *
     * The bridge is created here (it needs the `ComponentActivity`), but the
     * kernel reference lives in the composable, so the result is forwarded
     * through a settable [kernelSink] rather than a direct dependency.
     */
    private lateinit var signerBridge: ExternalSignerCapabilityBridge

    /** Set by [PodcastRoot] once the [KernelBridge] is constructed. */
    @Volatile
    private var kernelSink: ((String) -> Unit)? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        android.util.Log.i(
            "NmpSigner",
            "MainActivity.onCreate savedInstanceState=${savedInstanceState != null}",
        )
        signerBridge = ExternalSignerCapabilityBridge(this) { responseJson ->
            android.util.Log.i("NmpSigner", "kernelSink invoked, present=${kernelSink != null}")
            kernelSink?.invoke(responseJson)
        }
        signerBridge.register()
        setContent {
            MaterialTheme {
                PodcastRoot(
                    signerBridge = signerBridge,
                    setKernelSink = { sink -> kernelSink = sink },
                )
            }
        }
    }

    override fun onDestroy() {
        if (::signerBridge.isInitialized) signerBridge.unregister()
        kernelSink = null
        super.onDestroy()
    }
}

/**
 * Root composable — owns the [`KernelBridge`] lifecycle and the reactive
 * snapshot loop, then delegates rendering to [`AppNavigation`].
 *
 * Snapshot delivery is push-driven (issue #320): the loop blocks on
 * `bridge.nextUpdate()`, which drains frames delivered through the generated
 * UniFFI `PodcastUpdateSink`. It wakes the moment Rust emits a new frame and
 * otherwise costs nothing — no fixed timer, no repeated `podcastSnapshot()`
 * pulls. A single initial `podcastSnapshot()` pull paints the first frame so
 * the UI isn't blank until the kernel's first emit.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun PodcastRoot(
    signerBridge: ExternalSignerCapabilityBridge,
    setKernelSink: ((String) -> Unit) -> Unit,
) {
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
    // Per-domain rev tracker: mutable reference shared by the frame loop.
    // Each domain's rev is advanced when a sidecar is accepted; stale/duplicate
    // frames (rev <= last applied) are silently dropped per the drop-guard.
    val domainRevTracker = remember { DomainRevTracker() }
    var feedbackVisible by rememberSaveable { mutableStateOf(false) }

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
        // `KernelBridge` has already attached the app-domain Podcast handle);
        // the legacy `signinNsec` stub is intentionally NOT used — it feeds a
        // store the Identity screen never reads.
        KeystoreManager.loadNsec(context)?.let { stored ->
            IdentityActions.importNsec(bridge, stored)
        }
        ProviderCredentialActions.reloadProviderApiKeys(context, bridge)
        ProviderCredentialActions.syncSttKeysPresent(context, bridge)
        bridge.registerCapabilityRouter(router)
        // ADR-0048 — route Amber's raw results from the Activity-owned signer
        // bridge back into this composable's kernel. Installed after the kernel
        // exists; cleared on dispose so a torn-down kernel is never touched.
        setKernelSink { responseJson -> bridge.deliverSignerResponse(responseJson) }
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
        val firstRaw = withContext(Dispatchers.IO) { bridge.podcastSnapshot() }
        firstRaw?.let { raw ->
            SnapshotCodec.decode(raw)?.let { first ->
                snapshot = first
                download.reconcile(first.downloads?.active)
                if (first.activeAccount == null && KeystoreManager.loadNsec(context) == null) {
                    withContext(Dispatchers.IO) { IdentityActions.generate(bridge) }
                    val updatedRaw = withContext(Dispatchers.IO) { bridge.podcastSnapshot() }
                    updatedRaw?.let { SnapshotCodec.decode(it) }?.let { updated ->
                        snapshot = updated
                        download.reconcile(updated.downloads?.active)
                    }
                }
            }
        }

        // Steady state: block on the generated UniFFI update queue. `nextUpdate()`
        // returns the moment a new frame arrives — reactive, not timed. A `null`
        // return means the bridge is shutting down.
        //
        // NMP v0.5.0 per-domain push path: the slim `v` envelope carries only
        // `rev`/`running`/`schema_version`. The real domain payloads arrive as
        // typed sidecars under `v.projections["podcast.*"]`. We decode whichever
        // domains are present and MERGE them into the held snapshot via copy(),
        // leaving absent-domain slices untouched (no more whole-snapshot clobber).
        while (true) {
            coroutineContext.ensureActive()
            val raw = withContext(Dispatchers.IO) { bridge.nextUpdate() } ?: break
            val frames = SnapshotCodec.decodeDomainFrames(raw) ?: continue
            val current = snapshot ?: PodcastSnapshot()
            val (merged, anyAccepted) = SnapshotCodec.mergeFrames(frames, current, domainRevTracker)
            if (anyAccepted) {
                snapshot = merged
                download.reconcile(merged.downloads?.active)
            }
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

    // ADR-0048 — signer-request reader loop. Mirrors the snapshot loop above:
    // block on `nextSignerRequest()` and hand each `ExternalSignerRequest` JSON
    // to the Activity-owned bridge, which fires the Amber Intent. Reactive, not
    // timed: the channel is empty until a sign-in/sign op builds a request in Rust.
    LaunchedEffect(bridge, signerBridge) {
        while (true) {
            coroutineContext.ensureActive()
            val requestJson = withContext(Dispatchers.IO) { bridge.nextSignerRequest() } ?: break
            signerBridge.handleJson(requestJson)
        }
    }

    ShakeFeedbackDetector { feedbackVisible = true }

    // Explicit snapshot pull — used by screens that dispatch kernel actions
    // whose rev bump doesn't trigger an NMP-core push frame (e.g. Generate).
    val onSnapshotPull: suspend () -> Unit = {
        val raw = withContext(Dispatchers.IO) { bridge.podcastSnapshot() }
        raw?.let { SnapshotCodec.decode(it) }?.let { updated ->
            snapshot = updated
            download.reconcile(updated.downloads?.active)
        }
    }

    AppNavigation(
        snapshot = snapshot,
        bridge = bridge,
        // ADR-0048 — Amber is the default Android signer. Passing its package
        // explicitly lets Rust route the Intent (and pick the ContentResolver
        // fast-path post-grant); a null would let the OS resolver choose.
        onSignInWithAmber = { bridge.signInNip55(AMBER_SIGNER_PACKAGE) },
        onSnapshotPull = onSnapshotPull,
    )

    if (feedbackVisible) {
        ModalBottomSheet(onDismissRequest = { feedbackVisible = false }) {
            FeedbackSheet(
                snapshot = snapshot,
                bridge = bridge,
                onDismiss = { feedbackVisible = false },
                modifier = Modifier.fillMaxHeight(0.9f),
            )
        }
    }
}
