package io.f7z.podcast.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.f7z.podcast.ContactSummaryDto
import io.f7z.podcast.KernelBridge
import io.f7z.podcast.PodcastSnapshot
import io.f7z.podcast.ResolvedProfile

/**
 * Following screen — NIP-02 follow list with real display names and avatars
 * (iOS social parity, slice 2).
 *
 * Renders the kernel-projected follow list from `snapshot.following` (the
 * `podcast.social` domain frame's `SocialSnapshot.following` field). For each
 * contact, the screen calls `bridge.claimProfile(pubkeyHex)` on entry so the
 * kernel fetches kind:0 metadata and delivers it in
 * `snapshot.resolvedProfiles[pubkeyHex]`. Claims are released on dispose.
 *
 * Profile hydration mirrors [NostrConversationsScreen] exactly:
 *  - DisposableEffect on `following` keys claim/release on the kernel.
 *  - [NostrAvatar] renders the Coil-backed picture or placeholder.
 *  - Display name from `resolvedProfile.display`, falling back to
 *    `shortNpubLabel` when unresolved.
 *
 * LazyColumn key guards against duplicate/empty pubkeys: uses
 * `pubkeyHex.ifEmpty { npub }.ifEmpty { "$index" }`.
 *
 * Empty states:
 *  - Signed-out (`activeAccount == null`) — "Sign in to see your following list."
 *  - Signed-in but zero follows — "No accounts followed yet."
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun FollowingScreen(
    snapshot: PodcastSnapshot?,
    bridge: KernelBridge,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val isSignedIn = snapshot?.activeAccount != null
    val following = snapshot?.following ?: emptyList()
    val resolvedProfiles = snapshot?.resolvedProfiles ?: emptyMap()

    // Claim pubkeys so the kernel resolves kind:0 profiles and delivers them
    // in projections["resolved_profiles"]. Mirrors NostrConversationsScreen
    // claim/release lifecycle. Consumer ID is stable → kernel dedupes across
    // re-entries. Released on dispose so inflight requests are cancelled when
    // the screen leaves the composition.
    val consumerID = "FollowingScreen"
    DisposableEffect(following) {
        val claimed = following.map { it.pubkeyHex }.filter { it.isNotEmpty() }.distinct()
        claimed.forEach { pubkey -> bridge.claimProfile(pubkey, consumerID) }
        onDispose {
            claimed.forEach { pubkey -> bridge.releaseProfile(pubkey, consumerID) }
        }
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    val count = following.size
                    Text(if (count > 0) "Following ($count)" else "Following")
                },
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
        modifier = modifier,
    ) { inner ->
        when {
            !isSignedIn -> SignedOutEmptyState(Modifier.fillMaxSize().padding(inner))
            following.isEmpty() -> ZeroFollowsEmptyState(Modifier.fillMaxSize().padding(inner))
            else -> FollowingList(
                following = following,
                resolvedProfiles = resolvedProfiles,
                modifier = Modifier.fillMaxSize().padding(inner),
            )
        }
    }
}

@Composable
private fun FollowingList(
    following: List<ContactSummaryDto>,
    resolvedProfiles: Map<String, ResolvedProfile>,
    modifier: Modifier = Modifier,
) {
    LazyColumn(modifier = modifier) {
        itemsIndexed(
            items = following,
            // Guard against duplicate/empty pubkeys from malformed kind:3 events.
            key = { index, contact ->
                contact.pubkeyHex.ifEmpty { contact.npub }.ifEmpty { "$index" }
            },
        ) { _, contact ->
            val resolvedProfile = resolvedProfiles[contact.pubkeyHex]
            ContactRow(contact = contact, resolvedProfile = resolvedProfile)
            HorizontalDivider()
        }
    }
}

@Composable
private fun ContactRow(
    contact: ContactSummaryDto,
    resolvedProfile: ResolvedProfile?,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        // Avatar: show kernel-resolved picture via Coil; fall back to placeholder.
        NostrAvatar(
            pictureUrl = resolvedProfile?.pictureUrl,
            modifier = Modifier.size(40.dp),
        )

        Column(modifier = Modifier.weight(1f)) {
            // Primary label: resolved display name → short npub fallback.
            val displayLabel = resolvedProfile?.display?.takeIf { it.isNotBlank() }
                ?: contact.displayName?.takeIf { it.isNotBlank() }
                ?: shortNpubLabel(contact.npub)
            Text(
                text = displayLabel,
                style = MaterialTheme.typography.bodyLarge,
                fontWeight = FontWeight.Medium,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            // Secondary: show npub stub when a real name is present.
            if (resolvedProfile?.display != null || contact.displayName != null) {
                Text(
                    text = shortNpubLabel(contact.npub),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}

@Composable
private fun SignedOutEmptyState(modifier: Modifier = Modifier) {
    Box(modifier = modifier, contentAlignment = Alignment.Center) {
        Text(
            text = "Sign in to see your following list.",
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

@Composable
private fun ZeroFollowsEmptyState(modifier: Modifier = Modifier) {
    Box(modifier = modifier, contentAlignment = Alignment.Center) {
        Text(
            text = "No accounts followed yet.",
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

/**
 * Produces a short representation of a bech32 npub for display.
 *
 * For an npub like "npub1abc...xyz" produces "npub1abc…xyz" showing the first
 * 12 and last 8 characters, matching the iOS `NostrNpub.shortNpub` pattern.
 * Falls back to the raw string if it is too short to truncate.
 *
 * Named `shortNpubLabel` to avoid a package-level name collision with the
 * hex-variant `shortNpub` in `NostrConversationsScreen.kt`.
 */
private fun shortNpubLabel(npub: String): String {
    if (npub.length <= 20) return npub
    return "${npub.take(12)}…${npub.takeLast(8)}"
}
