package io.f7z.podcast.ui

import android.graphics.Bitmap
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.google.zxing.BarcodeFormat
import com.google.zxing.EncodeHintType
import com.google.zxing.qrcode.QRCodeWriter
import io.f7z.podcast.KernelBridge
import io.f7z.podcast.Nip46Uri
import io.f7z.podcast.PodcastSnapshot
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

// ── Pairing phase state machine ──────────────────────────────────────────────

/**
 * Phase of the nostrconnect:// QR pairing flow.
 * Defined at file scope so Kotlin can use sealed semantics without local-class
 * restrictions.
 */
internal sealed class NostrConnectPhase {
    data object Generating : NostrConnectPhase()
    data class Ready(val uri: String, val qr: Bitmap) : NostrConnectPhase()
    data object Connecting : NostrConnectPhase()
    data class Error(val msg: String) : NostrConnectPhase()
}

// ─────────────────────────────────────────────────────────────────────────────

/**
 * NIP-46 client-initiated pairing via `nostrconnect://`.
 *
 * Shows a QR code the user scans in a signer app (Amber, nsec.app, etc.).
 * The kernel generates the URI via `nmp_app_nostrconnect_uri`; the handshake
 * result surfaces reactively via the identity snapshot: a successful pairing
 * makes an EXTERNAL (remote-signer) active account appear, detected by
 * [Nip46Uri.handshakeCompleted] on the next push tick -- no polling, and no
 * string-matching on the diagnostic mode token (the projection emits "nip55"
 * for a kernel-owned bunker account, never a distinct "bunker" token).
 *
 * QR generation uses ZXing Core (`com.google.zxing:core:3.5.3`) -- the
 * lightweight encoder-only library. No ZXing Android Embedded UI dep is
 * required; we render the QR matrix directly into an `android.graphics.Bitmap`.
 * This mirrors iOS, which uses `CIFilter.qrCodeGenerator()`.
 *
 * States (mirror iOS NostrConnectView):
 *
 *  * Generating  -- spinner while `nostrconnectUri()` runs.
 *  * Idle (QR)   -- QR code displayed, waiting for signer.
 *  * Connecting  -- signer scanned; spinner.
 *  * Connected   -- navigates back to IdentityScreen (which shows SignedInState).
 *  * Error       -- kernel returned null URI; Retry button.
 *
 * [onBack] cancels any in-flight handshake before popping.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun NostrConnectScreen(
    snapshot: PodcastSnapshot?,
    bridge: KernelBridge,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var phase by remember { mutableStateOf<NostrConnectPhase>(NostrConnectPhase.Generating) }

    val account = snapshot?.activeAccount

    // Capture whether a remote-signer account already existed when this screen
    // appeared (normally false — NostrConnectScreen launches from the NIP-46
    // flow, which starts from NotSignedInState). A successful nostrconnect
    // handshake makes an EXTERNAL active account appear; that transition is the
    // completion signal. We do NOT string-match "bunker"/"nip46" — the Rust
    // identity projection never emits those tokens (a kernel-owned bunker
    // account surfaces as mode "nip55", i.e. external/remote signer). See
    // Nip46Uri for the rationale + the iOS `signer_is_remote` doctrine.
    val hadAccountAtStart = rememberSaveable {
        Nip46Uri.isRemoteSignerAccount(account)
    }
    val isPaired = Nip46Uri.handshakeCompleted(
        hadActiveAccountAtStart = hadAccountAtStart,
        current = account,
    )

    // Transition to connected: once the snapshot reflects the remote-signer
    // account, navigate back to IdentityScreen. It will show SignedInState with
    // the remote-signer mode badge -- mirrors iOS NostrConnectView
    // dismiss-on-connected.
    LaunchedEffect(isPaired) {
        if (isPaired) {
            onBack()
        }
    }

    // Generate the nostrconnect URI once when the screen appears.
    LaunchedEffect(Unit) {
        phase = NostrConnectPhase.Generating
        val uri = withContext(Dispatchers.IO) {
            // Pass null for both args: Rust selects the first write-capable
            // relay; no Android deep-link callback scheme registered yet.
            bridge.nostrconnectUri(relayUrl = null, callbackScheme = null)
        }
        phase = if (uri == null) {
            NostrConnectPhase.Error("Failed to generate connection URI. Is the kernel running?")
        } else {
            val qr = withContext(Dispatchers.Default) { makeQrBitmap(uri) }
            if (qr == null) NostrConnectPhase.Error("Failed to generate QR code.")
            else NostrConnectPhase.Ready(uri = uri, qr = qr)
        }
    }

    // Cancel handshake on dispose (e.g. back press before pairing completes).
    DisposableEffect(Unit) {
        onDispose {
            // D6 -- idempotent; safe when no handshake is in flight.
            bridge.cancelBunkerHandshake()
        }
    }

    Scaffold(
        modifier = modifier,
        topBar = {
            TopAppBar(
                title = { Text("Scan to connect") },
                navigationIcon = {
                    IconButton(onClick = {
                        bridge.cancelBunkerHandshake()
                        onBack()
                    }) {
                        Icon(
                            imageVector = Icons.AutoMirrored.Filled.ArrowBack,
                            contentDescription = "Back",
                        )
                    }
                },
            )
        },
    ) { inner ->
        Column(
            modifier = Modifier
                .padding(inner)
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 16.dp, vertical = 16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            // Preface -- mirrors iOS NostrConnectView.preface
            Text(
                text = "Open your signer app, tap \"Scan\" or \"New connection\", " +
                    "then point it at this code.",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface,
            )
            Text(
                text = "Your private key never touches this device.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )

            when (val p = phase) {
                is NostrConnectPhase.Generating -> {
                    Box(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(vertical = 32.dp),
                        contentAlignment = Alignment.Center,
                    ) {
                        Column(
                            horizontalAlignment = Alignment.CenterHorizontally,
                            verticalArrangement = Arrangement.spacedBy(12.dp),
                        ) {
                            CircularProgressIndicator()
                            Text(
                                text = "Generating...",
                                style = MaterialTheme.typography.bodyMedium,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                    }
                }

                is NostrConnectPhase.Ready -> {
                    NostrConnectQrSection(
                        qr = p.qr,
                        alpha = 1f,
                    )
                    Text(
                        text = "The QR code expires after 5 minutes. If it times out, " +
                            "come back to this screen to generate a new one.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.7f),
                    )
                    // UX helper: let user explicitly signal they've scanned.
                    // (The snapshot tick auto-advances when the signer connects.)
                    OutlinedButton(
                        onClick = { phase = NostrConnectPhase.Connecting },
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Text("I scanned it -- waiting for approval")
                    }
                }

                is NostrConnectPhase.Connecting -> {
                    Box(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(vertical = 16.dp),
                        contentAlignment = Alignment.Center,
                    ) {
                        Column(
                            horizontalAlignment = Alignment.CenterHorizontally,
                            verticalArrangement = Arrangement.spacedBy(12.dp),
                        ) {
                            CircularProgressIndicator()
                            Text(
                                text = "Waiting for signer to connect...",
                                style = MaterialTheme.typography.bodyMedium,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                    }
                    OutlinedButton(
                        onClick = {
                            bridge.cancelBunkerHandshake()
                            onBack()
                        },
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Text("Cancel")
                    }
                }

                is NostrConnectPhase.Error -> {
                    Column(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(12.dp),
                    ) {
                        Text(
                            text = p.msg,
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.error,
                        )
                        Button(
                            onClick = { phase = NostrConnectPhase.Generating },
                            modifier = Modifier.fillMaxWidth(),
                        ) {
                            Text("Try again")
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun NostrConnectQrSection(qr: Bitmap, alpha: Float) {
    Column(
        modifier = Modifier.fillMaxWidth(),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Box(
            modifier = Modifier
                .size(260.dp)
                .background(Color.White, shape = RoundedCornerShape(12.dp))
                .padding(16.dp)
                .alpha(alpha),
            contentAlignment = Alignment.Center,
        ) {
            Image(
                bitmap = qr.asImageBitmap(),
                contentDescription = "nostrconnect:// QR code",
                modifier = Modifier.fillMaxSize(),
            )
        }
    }
}

/**
 * Generate a square QR bitmap for [content] using ZXing Core.
 *
 * Returns `null` on any encode error (D6 -- the caller shows an error state).
 * Call from a coroutine on [Dispatchers.Default] (CPU-bound ZXing encode).
 * Mirror of iOS `NostrConnectView.makeQR(from:)`.
 */
internal fun makeQrBitmap(content: String, sizePx: Int = 800): Bitmap? {
    return try {
        val writer = QRCodeWriter()
        val hints = mapOf(EncodeHintType.MARGIN to 1)
        val matrix = writer.encode(content, BarcodeFormat.QR_CODE, sizePx, sizePx, hints)
        val bmp = Bitmap.createBitmap(sizePx, sizePx, Bitmap.Config.RGB_565)
        for (x in 0 until sizePx) {
            for (y in 0 until sizePx) {
                bmp.setPixel(
                    x, y,
                    if (matrix[x, y]) android.graphics.Color.BLACK
                    else android.graphics.Color.WHITE,
                )
            }
        }
        bmp
    } catch (_: Exception) {
        null
    }
}
