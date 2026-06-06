package io.f7z.podcast.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.f7z.podcast.EpisodeSummary
import io.f7z.podcast.KernelBridge

/**
 * Renders the Rust-owned playback queue. The list comes directly from
 * `PodcastSnapshot.queue`; all mutations dispatch canonical `podcast.queue`
 * or `podcast.player` actions and wait for the next snapshot tick.
 */
@Composable
fun QueueSection(
    queue: List<EpisodeSummary>,
    bridge: KernelBridge,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier,
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text(
                text = "Up Next",
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.SemiBold,
                modifier = Modifier.weight(1f),
            )
            if (queue.isNotEmpty()) {
                OutlinedButton(
                    onClick = {
                        PodcastActionDispatcher.dispatch(
                            bridge = bridge,
                            namespace = PodcastNamespace.PLAYER,
                            payload = PlayNextPayload(),
                        )
                    },
                ) {
                    Icon(Icons.Filled.PlayArrow, contentDescription = null, modifier = Modifier.size(18.dp))
                    Spacer(modifier = Modifier.size(8.dp))
                    Text("Play Next")
                }
                TextButton(
                    onClick = {
                        PodcastActionDispatcher.dispatch(
                            bridge = bridge,
                            namespace = PodcastNamespace.QUEUE,
                            payload = QueueClearPayload(),
                        )
                    },
                ) {
                    Text("Clear")
                }
            }
        }

        if (queue.isEmpty()) {
            Text(
                text = "Queue episodes from an episode page.",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(vertical = 8.dp),
            )
            return@Column
        }

        LazyColumn(
            modifier = Modifier.fillMaxWidth(),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            itemsIndexed(queue, key = { _, episode -> episode.id }) { index, episode ->
                QueueRow(
                    position = index + 1,
                    episode = episode,
                    onRemove = {
                        PodcastActionDispatcher.dispatch(
                            bridge = bridge,
                            namespace = PodcastNamespace.QUEUE,
                            payload = QueueRemovePayload(episodeId = episode.id),
                        )
                    },
                )
            }
        }
    }
}

@Composable
private fun QueueRow(position: Int, episode: EpisodeSummary, onRemove: () -> Unit) {
    ListItem(
        leadingContent = {
            Text(
                text = position.toString(),
                style = MaterialTheme.typography.labelLarge,
                color = MaterialTheme.colorScheme.primary,
            )
        },
        headlineContent = {
            Text(
                text = episode.title,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        },
        supportingContent = {
            Text(
                text = episode.podcastTitle ?: "Unknown show",
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        },
        trailingContent = {
            IconButton(onClick = onRemove) {
                Icon(Icons.Filled.Delete, contentDescription = "Remove from queue")
            }
        },
    )
}
