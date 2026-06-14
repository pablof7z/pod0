package io.f7z.podcast.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.QrCodeScanner
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import io.f7z.podcast.AccountSummary
import io.f7z.podcast.KernelBridge
import io.f7z.podcast.Nip46Uri
import io.f7z.podcast.PodcastSnapshot

/**
 * Remote-signer surface — NIP-46 bunker:// connect.
 *
 * Paste a `bunker://` URI (obtained from Amber, nsec.app, or any NIP-46
 * signer), tap Connect, and the kernel performs the async handshake. The
 * result surfaces reactively via the identity snapshot: a successful handshake
 * makes an EXTERNAL (remote-signer) active account appear, which
 * [Nip46Uri.handshakeCompleted] detects on the next push tick — no polling, and
 * no string-matching on the diagnostic mode token (the Rust projection never
 * emits "bunker"; a kernel-owned bunker account surfaces as mode "nip55").
 *
 * States (mirror iOS RemoteSignerView + Nip46ConnectCard):
 *
 *  * Idle        — URI input + Connect button.
 *  * Connecting  — progress indicator + Cancel.
 *  * Connected   — success card (reflected from snapshot) + Done.
 *  * Error       — shown when URI is empty on connect attempt.
 *
 * [onOpenNostrConnect] navigates to [NostrConnectScreen] (the QR / scan-to-
 * connect flow). [onBack] pops this surface.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun RemoteSignerScreen(
    snapshot: PodcastSnapshot?,
    bridge: KernelBridge,
    onBack: () -> Unit,
    onOpenNostrConnect: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var bunkerInput by remember { mutableStateOf("") }
    var isConnecting by remember { mutableStateOf(false) }
    var inputError by remember { mutableStateOf<String?>(null) }

    val account = snapshot?.activeAccount

    // Capture whether an active account already existed when this screen first
    // appeared. The NIP-46 screens launch from NotSignedInState, so this is
    // normally false. A successful bunker handshake makes an EXTERNAL active
    // account appear — that transition is the completion signal. We do NOT
    // string-match "bunker"/"nip46": the Rust identity projection never emits
    // those tokens (a kernel-owned bunker account surfaces as mode "nip55",
    // i.e. external/remote signer). See Nip46Uri for the full rationale and the
    // iOS `signer_is_remote` doctrine this mirrors.
    val hadAccountAtStart = rememberSaveable {
        Nip46Uri.isRemoteSignerAccount(account)
    }
    val isPaired = Nip46Uri.handshakeCompleted(
        hadActiveAccountAtStart = hadAccountAtStart,
        current = account,
    )

    // Clear connecting state once the snapshot reflects the paired account.
    LaunchedEffect(isPaired) {
        if (isPaired && isConnecting) {
            isConnecting = false
        }
    }

    Scaffold(
        modifier = modifier,
        topBar = {
            TopAppBar(
                title = { Text("Remote signer") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
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
            // Preface — mirrors iOS RemoteSignerView.preface
            Text(
                text = "Some people prefer to keep their key in a separate signing app " +
                    "— like Amber or nsec.app — and let other apps ask permission " +
                    "to post. This app supports that.",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface,
            )
            Text(
                text = "Open your signer app, find \"connect a new app\" (it might " +
                    "say \"bunker\"), and paste the link here.",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )

            // Scan-to-connect row — mirrors iOS RemoteSignerView.scanToConnectRow
            Card(
                modifier = Modifier.fillMaxWidth(),
                shape = RoundedCornerShape(12.dp),
                onClick = onOpenNostrConnect,
            ) {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(16.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    Icon(
                        imageVector = Icons.Filled.QrCodeScanner,
                        contentDescription = null,
                        modifier = Modifier.size(24.dp),
                        tint = MaterialTheme.colorScheme.primary,
                    )
                    Column(modifier = Modifier.weight(1f)) {
                        Text(
                            text = "Scan to connect",
                            style = MaterialTheme.typography.bodyLarge,
                            fontWeight = FontWeight.SemiBold,
                        )
                        Text(
                            text = "Generate a QR code your signer app can scan",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
            }

            // Divider
            androidx.compose.material3.HorizontalDivider()

            when {
                isPaired -> ConnectedSection(account = account, onDone = onBack)
                isConnecting -> ConnectingSection(
                    onCancel = {
                        bridge.cancelBunkerHandshake()
                        isConnecting = false
                    },
                )
                else -> BunkerInputSection(
                    bunkerInput = bunkerInput,
                    onInputChange = { bunkerInput = it; inputError = null },
                    inputError = inputError,
                    onConnect = {
                        val trimmed = bunkerInput.trim()
                        if (trimmed.isEmpty()) {
                            inputError = "Paste a bunker:// URI from your signer app."
                        } else if (!Nip46Uri.isPlausibleBunkerUri(trimmed)) {
                            inputError = "URI must start with bunker://"
                        } else {
                            inputError = null
                            isConnecting = true
                            bridge.signInBunker(trimmed, makeActive = true)
                        }
                    },
                )
            }

            Spacer(modifier = Modifier.height(8.dp))

            // Footnote — mirrors iOS RemoteSignerView.footnote
            Text(
                text = "Your private key never touches this device — every signature " +
                    "happens inside your signer app.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.7f),
            )
        }
    }
}

@Composable
private fun BunkerInputSection(
    bunkerInput: String,
    onInputChange: (String) -> Unit,
    inputError: String?,
    onConnect: () -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        OutlinedTextField(
            value = bunkerInput,
            onValueChange = onInputChange,
            label = { Text("bunker:// URI") },
            placeholder = { Text("bunker://...") },
            isError = inputError != null,
            supportingText = inputError?.let { { Text(it) } },
            modifier = Modifier.fillMaxWidth(),
            singleLine = false,
            minLines = 2,
            maxLines = 5,
            keyboardOptions = KeyboardOptions(
                keyboardType = KeyboardType.Uri,
                imeAction = ImeAction.Done,
            ),
            keyboardActions = KeyboardActions(onDone = { onConnect() }),
        )
        Button(
            onClick = onConnect,
            modifier = Modifier.fillMaxWidth(),
            enabled = bunkerInput.isNotBlank(),
        ) {
            Text("Connect")
        }
    }
}

@Composable
private fun ConnectingSection(onCancel: () -> Unit) {
    Column(
        modifier = Modifier.fillMaxWidth(),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        CircularProgressIndicator()
        Text(
            text = "Connecting to remote signer...",
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        OutlinedButton(
            onClick = onCancel,
            modifier = Modifier.fillMaxWidth(),
        ) {
            Text("Cancel")
        }
    }
}

@Composable
private fun ConnectedSection(account: AccountSummary?, onDone: () -> Unit) {
    Column(
        modifier = Modifier.fillMaxWidth(),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Icon(
            imageVector = Icons.Filled.CheckCircle,
            contentDescription = null,
            modifier = Modifier.size(56.dp),
            tint = MaterialTheme.colorScheme.primary,
        )
        Text(
            text = "Connected",
            style = MaterialTheme.typography.headlineSmall,
            fontWeight = FontWeight.SemiBold,
        )
        val displayName = account?.displayName?.takeIf { it.isNotBlank() }
        if (displayName != null) {
            Text(
                text = displayName,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Text(
            text = "Your signer app is linked. You can close this.",
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Button(
            onClick = onDone,
            modifier = Modifier.fillMaxWidth(),
        ) {
            Text("Done")
        }
    }
}
