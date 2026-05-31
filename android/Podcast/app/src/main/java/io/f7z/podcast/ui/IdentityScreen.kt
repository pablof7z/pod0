package io.f7z.podcast.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.AssistChip
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.f7z.podcast.AccountSummary
import io.f7z.podcast.IdentityActions
import io.f7z.podcast.KernelBridge
import io.f7z.podcast.PodcastSnapshot
import io.f7z.podcast.security.KeystoreManager

/**
 * Identity surface — shows the active Nostr account (when present) or a
 * "Not signed in" state with a real local-key import flow.
 *
 * Bound directly to `snapshot.activeAccount`. Sign-in imports an `nsec` via
 * the `podcast.identity` `ImportNsec` action ([IdentityActions]) and persists
 * it to the Android Keystore ([KeystoreManager]); sign-out dispatches `Clear`
 * and wipes the stored key. The legacy `bridge.signinNsec` stub is NOT used —
 * it feeds the nmp-core multi-account store that `activeAccount` never reads.
 *
 * NIP-46 bunker sign-in and key generation are out of scope here (see the PR
 * notes): the kernel `Generate` action does not return the new nsec to the
 * host, so it cannot be persisted on-device.
 *
 * `ModeBadge` mirrors the iOS `Features/Identity/ModeBadge.swift` surface — a
 * small pill marking which auth mode the user is in.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun IdentityScreen(
    snapshot: PodcastSnapshot?,
    bridge: KernelBridge,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Scaffold(
        modifier = modifier,
        topBar = {
            TopAppBar(
                title = { Text("Identity") },
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
        val account = snapshot?.activeAccount
        Column(
            modifier = Modifier
                .padding(inner)
                .fillMaxSize()
                .padding(horizontal = 16.dp, vertical = 16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            if (account == null) {
                NotSignedInState(bridge = bridge)
            } else {
                SignedInState(account = account, bridge = bridge)
            }
        }
    }
}

@Composable
private fun SignedInState(account: AccountSummary, bridge: KernelBridge) {
    val context = LocalContext.current
    var confirmSignOut by remember { mutableStateOf(false) }

    Card(modifier = Modifier.fillMaxWidth()) {
        Row(
            modifier = Modifier.padding(16.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            AvatarPlaceholder()
            Column(modifier = Modifier.fillMaxWidth()) {
                Text(
                    text = account.displayName?.takeIf { it.isNotBlank() } ?: "Anonymous",
                    style = MaterialTheme.typography.titleLarge,
                    fontWeight = FontWeight.SemiBold,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    text = abbreviateNpub(account.npub),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                )
            }
        }
    }
    ModeBadge(mode = account.mode)
    OutlinedButton(
        onClick = { confirmSignOut = true },
        modifier = Modifier.fillMaxWidth(),
    ) { Text("Sign out") }

    if (confirmSignOut) {
        AlertDialog(
            onDismissRequest = { confirmSignOut = false },
            title = { Text("Sign out?") },
            text = {
                Text("Your private key will be removed from this device. You'll need your nsec to sign back in.")
            },
            confirmButton = {
                TextButton(
                    onClick = {
                        // Clear the kernel identity (-> activeAccount becomes null)
                        // then wipe the on-device key. Order matters only for the
                        // crash window between the two; both are idempotent.
                        IdentityActions.clear(bridge)
                        KeystoreManager.clearNsec(context)
                        confirmSignOut = false
                    },
                ) { Text("Sign out") }
            },
            dismissButton = {
                TextButton(onClick = { confirmSignOut = false }) { Text("Cancel") }
            },
        )
    }
}

@Composable
private fun NotSignedInState(bridge: KernelBridge) {
    val context = LocalContext.current
    var showSheet by remember { mutableStateOf(false) }

    Card(modifier = Modifier.fillMaxWidth()) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(24.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Text(
                text = "Not signed in",
                style = MaterialTheme.typography.titleLarge,
                fontWeight = FontWeight.SemiBold,
            )
            Text(
                text = "Sign in with Nostr to sync subscriptions, briefings, and listening history across devices.",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Button(onClick = { showSheet = true }) { Text("Import nsec key") }
        }
    }

    if (showSheet) {
        ImportNsecSheet(
            onDismiss = { showSheet = false },
            onSubmit = { nsec ->
                // Dispatch the canonical podcast.identity ImportNsec (populates
                // activeAccount on the next snapshot) and persist the key so it
                // survives restart. The screen re-renders to SignedInState when
                // the snapshot reports the new activeAccount.
                IdentityActions.importNsec(bridge, nsec)
                KeystoreManager.saveNsec(context, nsec)
                showSheet = false
            },
        )
    }
}

/**
 * Local pill matching the iOS `ModeBadge`. Tagging visible "Local Key"
 * vs "Bunker" so the user knows whether the kernel holds the private
 * key directly or proxies through an external signer.
 */
@Composable
private fun ModeBadge(mode: String) {
    val label = when (mode.lowercase()) {
        "bunker", "nip46", "nip-46" -> "Bunker"
        else -> "Local Key"
    }
    AssistChip(
        onClick = { /* read-only badge — no action */ },
        label = { Text(label) },
    )
}

@Composable
private fun AvatarPlaceholder() {
    Surface(
        modifier = Modifier
            .size(56.dp)
            .clip(RoundedCornerShape(28.dp)),
        color = MaterialTheme.colorScheme.surfaceVariant,
    ) {
        Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            Text(
                text = "▲",
                style = MaterialTheme.typography.titleMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

/**
 * Render the first 16 chars of the npub + an ellipsis. Mirrors the
 * abbreviated form iOS shows in `AccountDetailsView`. Falls back to the
 * raw string when shorter than the prefix.
 */
private fun abbreviateNpub(npub: String): String {
    if (npub.length <= 16) return npub
    return "${npub.take(16)}…"
}
