package io.f7z.podcast.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Forum
import androidx.compose.material.icons.filled.Person
import androidx.compose.material.icons.filled.Shield
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import coil.compose.AsyncImage
import io.f7z.podcast.KernelBridge
import io.f7z.podcast.NostrConversationDto
import io.f7z.podcast.PodcastSnapshot
import io.f7z.podcast.ResolvedProfile
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.util.concurrent.TimeUnit

/**
 * Nostr Conversations list screen — iOS parity for `NostrConversationsView.swift`.
 *
 * Renders every NIP-10-threaded Nostr conversation the agent has participated
 * in. Conversations come from `snapshot.nostrConversations` (the
 * `podcast.social` domain frame), sorted newest-first by `lastActivity`.
 *
 * Profile names hydrate from the kernel-projected data in the conversation
 * (display name is not separately resolved on Android yet; the short npub hex
 * is shown as fallback, matching the iOS `NostrNpub.shortNpub(fromHex:)` path
 * when no profile cache entry is present).
 *
 * Navigation: reached from Settings → "Nostr Conversations". Tapping a row
 * pushes [NostrConversationDetailScreen] via [onConversationSelected].
 *
 * Wire contract consumed: `SocialDomainFrame.nostrConversations`
 * (`List<NostrConversationDto>`). No actions dispatched — read-only surface.
 *
 * D5/D8: all state from kernel; no local derivation beyond sort + display.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun NostrConversationsScreen(
    snapshot: PodcastSnapshot?,
    bridge: KernelBridge,
    onConversationSelected: (NostrConversationDto) -> Unit,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val conversations = snapshot?.nostrConversations
        ?.sortedByDescending { it.lastActivity }
        ?: emptyList()

    val resolvedProfiles = snapshot?.resolvedProfiles ?: emptyMap()

    // Claim all counterparty pubkeys so the kernel resolves their kind:0
    // profiles and delivers them in `projections["resolved_profiles"]`. Mirrors
    // iOS `.claimNostrProfiles(_:consumer:)` lifecycle. The consumer ID is
    // stable for this screen so the kernel's refcount dedupes across re-entries.
    // Released on dispose so the kernel can drop inflight requests when the
    // screen leaves the composition.
    val consumerID = "NostrConversationsScreen"
    DisposableEffect(conversations) {
        val claimed = conversations.map { it.counterpartyHex }.distinct()
        claimed.forEach { pubkey -> bridge.claimProfile(pubkey, consumerID) }
        onDispose {
            claimed.forEach { pubkey -> bridge.releaseProfile(pubkey, consumerID) }
        }
    }

    Scaffold(
        modifier = modifier,
        topBar = {
            TopAppBar(
                title = { Text("Conversations") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
            )
        },
    ) { innerPadding ->
        if (conversations.isEmpty()) {
            NostrConversationsEmptyState(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(innerPadding),
            )
        } else {
            LazyColumn(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(innerPadding),
            ) {
                items(conversations, key = { it.rootEventId }) { conv ->
                    NostrConversationRow(
                        conv = conv,
                        resolvedProfile = resolvedProfiles[conv.counterpartyHex],
                        onClick = { onConversationSelected(conv) },
                    )
                    HorizontalDivider(modifier = Modifier.padding(start = 72.dp))
                }
            }
        }
    }
}

// ── Row ───────────────────────────────────────────────────────────────────────

@Composable
private fun NostrConversationRow(
    conv: NostrConversationDto,
    resolvedProfile: ResolvedProfile?,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val inboundCount = conv.turns.count { it.direction == "inbound" }
    val outboundCount = conv.turns.count { it.direction == "outbound" }
    val lastTurn = conv.turns.lastOrNull()
    // Show resolved display name when available; fall back to shortHex.
    val primaryLabel = resolvedProfile?.display ?: shortHex(conv.counterpartyHex)

    Row(
        modifier = modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalAlignment = Alignment.Top,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        // Avatar slot: show the kernel-resolved picture URL when available via
        // Coil AsyncImage; fall back to the placeholder on unresolved profiles.
        NostrAvatar(
            pictureUrl = resolvedProfile?.pictureUrl,
            modifier = Modifier.size(40.dp),
        )

        Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            // Header row: display name + timestamp
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(4.dp),
                    modifier = Modifier.weight(1f),
                ) {
                    Text(
                        text = primaryLabel,
                        style = MaterialTheme.typography.titleSmall,
                        fontWeight = FontWeight.SemiBold,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                    if (conv.trusted) {
                        Icon(
                            imageVector = Icons.Filled.Shield,
                            contentDescription = "Trusted",
                            modifier = Modifier.size(14.dp),
                            tint = MaterialTheme.colorScheme.primary,
                        )
                    }
                }
                Text(
                    text = relativeTime(conv.lastActivity),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }

            // Short npub under the primary label (mirrors iOS secondaryLabel behaviour)
            Text(
                text = shortNpub(conv.counterpartyHex),
                style = MaterialTheme.typography.bodySmall,
                fontFamily = androidx.compose.ui.text.font.FontFamily.Monospace,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )

            // Last turn preview
            if (lastTurn != null && lastTurn.content.isNotBlank()) {
                Text(
                    text = lastTurn.content,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
            }

            // Turn count badge row — mirrors iOS "N in / N out" pill
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                TurnCountChip(label = "$inboundCount in")
                TurnCountChip(label = "$outboundCount out")
            }
        }
    }
}

// ── Avatar composables ────────────────────────────────────────────────────────

/**
 * Circular Nostr avatar: shows the kernel-resolved [pictureUrl] via Coil
 * [AsyncImage] when available; falls back to [NostrAvatarPlaceholder] on
 * unresolved profiles or load failures.
 *
 * This replaces the old hard-coded [NostrAvatarPlaceholder] call sites now
 * that the kernel delivers resolved_profiles via the claim/release seam.
 */
@Composable
fun NostrAvatar(
    pictureUrl: String?,
    modifier: Modifier = Modifier,
) {
    if (!pictureUrl.isNullOrBlank()) {
        AsyncImage(
            model = pictureUrl,
            contentDescription = null,
            modifier = modifier.clip(CircleShape),
        )
    } else {
        NostrAvatarPlaceholder(modifier = modifier)
    }
}

/**
 * Circular person-icon placeholder used when no kernel-resolved profile
 * picture is available (unresolved pubkey, or profile has no picture).
 */
@Composable
fun NostrAvatarPlaceholder(modifier: Modifier = Modifier) {
    Surface(
        modifier = modifier.clip(CircleShape),
        shape = CircleShape,
        color = MaterialTheme.colorScheme.surfaceVariant,
    ) {
        Box(contentAlignment = Alignment.Center) {
            Icon(
                imageVector = Icons.Filled.Person,
                contentDescription = null,
                modifier = Modifier.size(24.dp),
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

// ── Turn count chip ───────────────────────────────────────────────────────────

@Composable
private fun TurnCountChip(label: String) {
    Surface(
        color = MaterialTheme.colorScheme.secondaryContainer,
        shape = MaterialTheme.shapes.small,
    ) {
        Text(
            text = label,
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSecondaryContainer,
            modifier = Modifier.padding(horizontal = 6.dp, vertical = 2.dp),
        )
    }
}

// ── Empty state ───────────────────────────────────────────────────────────────

@Composable
private fun NostrConversationsEmptyState(modifier: Modifier = Modifier) {
    Box(modifier = modifier, contentAlignment = Alignment.Center) {
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(8.dp),
            modifier = Modifier.padding(32.dp),
        ) {
            Icon(
                imageVector = Icons.Filled.Forum,
                contentDescription = null,
                modifier = Modifier.size(48.dp),
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Spacer(modifier = Modifier.width(0.dp))
            Text(
                text = "No conversations yet",
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.SemiBold,
            )
            Text(
                text = "When your agent receives a Nostr mention and replies, the thread will appear here.",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                textAlign = androidx.compose.ui.text.style.TextAlign.Center,
            )
        }
    }
}

// ── Formatting helpers ────────────────────────────────────────────────────────

/**
 * Returns the first 8 characters of the hex pubkey as a display label —
 * mirrors iOS `NostrNpub.shortNpub(fromHex:)`.
 */
internal fun shortHex(hexPubkey: String): String =
    if (hexPubkey.length > 8) hexPubkey.take(8) + "..." else hexPubkey

/**
 * Secondary label: "npub1" prefix + first 6 of hex to approximate the npub
 * short form. iOS does a real bech32 conversion; we skip the dep and render
 * the raw hex abbreviation instead — functionally equivalent for disambiguation.
 */
internal fun shortNpub(hexPubkey: String): String =
    if (hexPubkey.length > 8) "npub1…${hexPubkey.takeLast(6)}" else hexPubkey

/**
 * Relative time string for a Unix-seconds timestamp — mirrors iOS
 * `RelativeDateTimeFormatter`. Falls back to a formatted date for anything
 * older than 7 days.
 */
internal fun relativeTime(epochSeconds: Long): String {
    val nowMs = System.currentTimeMillis()
    val diffMs = nowMs - epochSeconds * 1_000L
    return when {
        diffMs < 0 -> "just now"
        diffMs < TimeUnit.MINUTES.toMillis(1) -> "now"
        diffMs < TimeUnit.HOURS.toMillis(1) -> {
            val m = TimeUnit.MILLISECONDS.toMinutes(diffMs)
            "${m}m ago"
        }
        diffMs < TimeUnit.DAYS.toMillis(1) -> {
            val h = TimeUnit.MILLISECONDS.toHours(diffMs)
            "${h}h ago"
        }
        diffMs < TimeUnit.DAYS.toMillis(7) -> {
            val d = TimeUnit.MILLISECONDS.toDays(diffMs)
            "${d}d ago"
        }
        else -> SimpleDateFormat("MMM d", Locale.getDefault()).format(Date(epochSeconds * 1_000L))
    }
}
