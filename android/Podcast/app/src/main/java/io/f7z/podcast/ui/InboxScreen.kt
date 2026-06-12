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
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.FilledIconButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.IconButtonDefaults
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import coil.compose.AsyncImage
import io.f7z.podcast.InboxItem
import io.f7z.podcast.KernelBridge
import io.f7z.podcast.PodcastSnapshot
import java.text.DateFormat
import java.util.Date

/**
 * Inbox tab — AI-triaged "what to listen to next" queue.
 *
 * Renders the `snapshot.inbox` list (priority-ordered `InboxItem`s) that
 * the kernel projects from its triage engine.
 *
 * Triage shimmer: while `snapshot.inboxTriageInProgress` is true a
 * [LinearProgressIndicator] runs below the heading so the user knows scoring
 * is in progress. The indicator disappears once the flag clears.
 *
 * Per-row affordances:
 *  - Play  → `podcast.player` `{"op":"play","episode_id":"…"}`
 *  - Dismiss → `podcast.inbox` `{"op":"dismiss","episode_id":"…"}`
 *  - Mark listened → `podcast.inbox` `{"op":"mark_listened","episode_id":"…"}`
 *
 * D8: no policy — every action is dispatched to the kernel; the next
 * snapshot tick reflects the result.
 */
@Composable
fun InboxScreen(
    snapshot: PodcastSnapshot?,
    bridge: KernelBridge,
    modifier: Modifier = Modifier,
) {
    val inbox = snapshot?.inbox ?: emptyList()
    val triageRunning = snapshot?.inboxTriageInProgress ?: false

    LazyColumn(
        modifier = modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
        contentPadding = androidx.compose.foundation.layout.PaddingValues(vertical = 16.dp),
    ) {
        item {
            InboxHeader(triageRunning = triageRunning, bridge = bridge)
        }

        if (inbox.isEmpty()) {
            item { InboxEmptyState(triageRunning = triageRunning) }
        } else {
            items(inbox, key = { it.episodeId }) { item ->
                InboxItemRow(item = item, bridge = bridge)
            }
        }
    }
}

@Composable
private fun InboxHeader(triageRunning: Boolean, bridge: KernelBridge) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Text(
                text = "Inbox",
                style = MaterialTheme.typography.headlineMedium,
                fontWeight = FontWeight.SemiBold,
            )
            if (triageRunning) {
                CircularProgressIndicator(modifier = Modifier.size(20.dp), strokeWidth = 2.dp)
            } else {
                IconButton(
                    onClick = {
                        PodcastActionDispatcher.dispatch(
                            bridge = bridge,
                            namespace = PodcastNamespace.INBOX,
                            payload = InboxTriagePayload(),
                        )
                    },
                ) {
                    Icon(
                        imageVector = Icons.Filled.Check,
                        contentDescription = "Refresh triage",
                        tint = MaterialTheme.colorScheme.primary,
                    )
                }
            }
        }
        if (triageRunning) {
            LinearProgressIndicator(modifier = Modifier.fillMaxWidth())
        }
    }
}

@Composable
private fun InboxItemRow(item: InboxItem, bridge: KernelBridge) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceVariant,
        ),
    ) {
        Row(
            modifier = Modifier.padding(12.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            InboxItemArtwork(artworkUrl = item.artworkUrl)
            Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(
                    text = item.episodeTitle,
                    style = MaterialTheme.typography.titleSmall,
                    fontWeight = FontWeight.Medium,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    text = item.podcastTitle,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                InboxItemMeta(item = item)
                item.priorityReason?.takeIf { it.isNotBlank() }?.let { reason ->
                    Text(
                        text = reason,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.primary,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
            InboxItemActions(item = item, bridge = bridge)
        }
    }
}

@Composable
private fun InboxItemArtwork(artworkUrl: String?) {
    val shape = RoundedCornerShape(8.dp)
    if (artworkUrl.isNullOrBlank()) {
        ArtworkPlaceholder(size = 56)
    } else {
        AsyncImage(
            model = artworkUrl,
            contentDescription = null,
            modifier = Modifier.size(56.dp).clip(shape),
        )
    }
}

@Composable
private fun InboxItemMeta(item: InboxItem) {
    val parts = buildList {
        item.durationSecs?.let { secs -> formatInboxDuration(secs).takeIf { it.isNotBlank() }?.let(::add) }
        if (item.publishedAt > 0) add(formatInboxDate(item.publishedAt))
    }
    if (parts.isEmpty()) return
    Text(
        text = parts.joinToString("  •  "),
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
}

@Composable
private fun InboxItemActions(item: InboxItem, bridge: KernelBridge) {
    Column(
        verticalArrangement = Arrangement.spacedBy(4.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        FilledIconButton(
            onClick = {
                PodcastActionDispatcher.dispatch(
                    bridge = bridge,
                    namespace = PodcastNamespace.PLAYER,
                    payload = PlayPayload(episodeId = item.episodeId),
                )
            },
        ) {
            Icon(Icons.Filled.PlayArrow, contentDescription = "Play")
        }
        IconButton(
            onClick = {
                PodcastActionDispatcher.dispatch(
                    bridge = bridge,
                    namespace = PodcastNamespace.INBOX,
                    payload = InboxMarkListenedPayload(episodeId = item.episodeId),
                )
            },
            colors = IconButtonDefaults.iconButtonColors(
                contentColor = MaterialTheme.colorScheme.secondary,
            ),
        ) {
            Icon(Icons.Filled.Check, contentDescription = "Mark listened", modifier = Modifier.size(18.dp))
        }
        IconButton(
            onClick = {
                PodcastActionDispatcher.dispatch(
                    bridge = bridge,
                    namespace = PodcastNamespace.INBOX,
                    payload = InboxDismissPayload(episodeId = item.episodeId),
                )
            },
            colors = IconButtonDefaults.iconButtonColors(
                contentColor = MaterialTheme.colorScheme.onSurfaceVariant,
            ),
        ) {
            Icon(Icons.Filled.Close, contentDescription = "Dismiss", modifier = Modifier.size(18.dp))
        }
    }
}

@Composable
private fun InboxEmptyState(triageRunning: Boolean) {
    Box(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 48.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            text = if (triageRunning) "Scoring episodes…" else "Inbox is empty",
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

private fun formatInboxDuration(durationSecs: Double): String {
    if (durationSecs <= 0) return ""
    val totalMinutes = (durationSecs / 60.0).toInt()
    val hours = totalMinutes / 60
    val minutes = totalMinutes % 60
    return if (hours > 0) "${hours}h ${minutes}m" else "$minutes min"
}

private fun formatInboxDate(unixSeconds: Long): String =
    DateFormat.getDateInstance(DateFormat.MEDIUM).format(Date(unixSeconds * 1000L))
