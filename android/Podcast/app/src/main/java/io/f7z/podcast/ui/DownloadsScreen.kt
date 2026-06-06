package io.f7z.podcast.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material3.Card
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.f7z.podcast.DownloadItemSnapshot
import io.f7z.podcast.EpisodeSummary
import io.f7z.podcast.KernelBridge
import io.f7z.podcast.PodcastSnapshot

/**
 * Downloads tab — active download queue + completed offline episodes.
 *
 * Two data sources, both read straight off the live snapshot (D5/D8 — no
 * local copy of download state):
 *
 *  * **Active** rows come from `snapshot.downloads.active`
 *    ([DownloadItemSnapshot]). That row carries only `episode_id` + progress,
 *    so we join it against the library episodes by id to recover the title
 *    and show name. Completed items drop out of `active` on the Rust side,
 *    so this section is strictly in-flight / queued / paused / failed.
 *  * **Downloaded** rows are library [EpisodeSummary]s whose `download_path`
 *    is non-null — there is no separate "completed downloads" list in the
 *    projection; a finished download is signalled solely by `download_path`.
 *
 * Mutations are kernel-bound:
 *  * Cancel an active download → `podcast.player` `{"op":"cancel_download",…}`
 *    (NOT `delete_download`, which only unlinks a finished file).
 *  * Delete a completed download → `podcast` `{"op":"delete_download",…}`.
 *  * Play a downloaded episode → `podcast.player` `{"op":"play",…}`.
 *
 * Android owns a real OkHttp `DownloadCapability`, but the screen still reads
 * only kernel state: active progress from `downloads.active` and completed
 * files from `EpisodeSummary.download_path`. The projection exposes total
 * bytes for active rows; completed rows use the episode's persisted file-size
 * field when present.
 */
@Composable
fun DownloadsScreen(
    snapshot: PodcastSnapshot?,
    bridge: KernelBridge,
    modifier: Modifier = Modifier,
) {
    val episodesById = rememberEpisodeIndex(snapshot)
    val active = snapshot?.downloads?.active ?: emptyList()
    val downloaded = episodesById.values.filter { !it.downloadPath.isNullOrBlank() }

    if (active.isEmpty() && downloaded.isEmpty()) {
        DownloadsEmptyState(modifier = modifier)
        return
    }

    LazyColumn(
        modifier = modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
        contentPadding = PaddingValues(vertical = 16.dp),
    ) {
        item {
            Text(
                text = "Downloads",
                style = MaterialTheme.typography.headlineMedium,
                fontWeight = FontWeight.SemiBold,
            )
        }

        if (active.isNotEmpty()) {
            item { SectionLabel("Downloading") }
            items(active, key = { "active-${it.episodeId}" }) { item ->
                ActiveDownloadRow(
                    item = item,
                    episode = episodesById[item.episodeId],
                    onCancel = {
                        PodcastActionDispatcher.dispatch(
                            bridge = bridge,
                            namespace = PodcastNamespace.PLAYER,
                            payload = CancelDownloadPayload(episodeId = item.episodeId),
                        )
                    },
                )
            }
        }

        if (downloaded.isNotEmpty()) {
            item { SectionLabel("Downloaded") }
            items(downloaded, key = { "done-${it.id}" }) { episode ->
                DownloadedEpisodeRow(
                    episode = episode,
                    onPlay = {
                        PodcastActionDispatcher.dispatch(
                            bridge = bridge,
                            namespace = PodcastNamespace.PLAYER,
                            payload = PlayPayload(episodeId = episode.id),
                        )
                    },
                    onDelete = {
                        PodcastActionDispatcher.dispatch(
                            bridge = bridge,
                            namespace = PodcastNamespace.PODCAST,
                            payload = DownloadDeletePayload(episodeId = episode.id),
                        )
                    },
                )
            }
        }
    }
}

@Composable
private fun SectionLabel(text: String) {
    Text(
        text = text.uppercase(),
        style = MaterialTheme.typography.labelMedium,
        fontWeight = FontWeight.SemiBold,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = Modifier.padding(start = 4.dp),
    )
}

@Composable
private fun ActiveDownloadRow(
    item: DownloadItemSnapshot,
    episode: EpisodeSummary?,
    onCancel: () -> Unit,
) {
    Card(modifier = Modifier.fillMaxWidth()) {
        Row(
            modifier = Modifier.padding(12.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Column(
                modifier = Modifier.fillMaxWidth(0.82f),
                verticalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                Text(
                    text = episode?.title ?: "Episode ${item.episodeId.take(8)}",
                    style = MaterialTheme.typography.titleSmall,
                    fontWeight = FontWeight.Medium,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
                val subtitle = episode?.podcastTitle
                if (!subtitle.isNullOrBlank()) {
                    Text(
                        text = subtitle,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                if (item.state == "failed") {
                    Text(
                        text = item.error?.let { "Failed: $it" } ?: "Download failed",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.error,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                    )
                } else {
                    LinearProgressIndicator(
                        progress = { item.progress.coerceIn(0f, 1f) },
                        modifier = Modifier.fillMaxWidth(),
                    )
                    Text(
                        text = activeStatusLabel(item),
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
            IconButton(onClick = onCancel) {
                Icon(
                    imageVector = Icons.Filled.Close,
                    contentDescription = "Cancel download",
                    tint = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}

@Composable
private fun DownloadedEpisodeRow(
    episode: EpisodeSummary,
    onPlay: () -> Unit,
    onDelete: () -> Unit,
) {
    Card(modifier = Modifier.fillMaxWidth()) {
        Row(
            modifier = Modifier.padding(12.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Column(
                modifier = Modifier.fillMaxWidth(0.72f),
                verticalArrangement = Arrangement.spacedBy(2.dp),
            ) {
                Text(
                    text = episode.title,
                    style = MaterialTheme.typography.titleSmall,
                    fontWeight = FontWeight.Medium,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
                val subtitle = episode.podcastTitle
                if (!subtitle.isNullOrBlank()) {
                    Text(
                        text = subtitle,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                Text(
                    text = "Downloaded",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.primary,
                )
            }
            IconButton(onClick = onPlay) {
                Icon(
                    imageVector = Icons.Filled.PlayArrow,
                    contentDescription = "Play",
                    tint = MaterialTheme.colorScheme.primary,
                )
            }
            IconButton(onClick = onDelete) {
                Icon(
                    imageVector = Icons.Filled.Delete,
                    contentDescription = "Delete download",
                    tint = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}

/**
 * Per-episode download affordance reused by ShowDetail + EpisodeDetail.
 *
 * Renders one of three states, derived entirely from the live snapshot:
 *  * **Downloaded** (`episode.download_path != null`) → "Downloaded" label +
 *    a delete icon dispatching `podcast` `{"op":"delete_download",…}`.
 *  * **In flight** (an `active` row exists for this id) → a compact progress
 *    indicator + a cancel icon dispatching `podcast.player`
 *    `{"op":"cancel_download",…}`.
 *  * **Not downloaded** → a "Download" button dispatching `podcast`
 *    `{"op":"download",…}`.
 *
 * `activeItem` is the matching [DownloadItemSnapshot] (or null); callers look
 * it up from `snapshot.downloads.active` so this composable stays snapshot-pure.
 */
@Composable
fun EpisodeDownloadButton(
    episode: EpisodeSummary,
    activeItem: DownloadItemSnapshot?,
    bridge: KernelBridge,
    modifier: Modifier = Modifier,
) {
    val downloaded = !episode.downloadPath.isNullOrBlank()
    when {
        downloaded -> Row(
            modifier = modifier,
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Text(
                text = "Downloaded",
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.primary,
            )
            IconButton(onClick = {
                PodcastActionDispatcher.dispatch(
                    bridge = bridge,
                    namespace = PodcastNamespace.PODCAST,
                    payload = DownloadDeletePayload(episodeId = episode.id),
                )
            }) {
                Icon(
                    imageVector = Icons.Filled.Delete,
                    contentDescription = "Delete download",
                    tint = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }

        activeItem != null -> Row(
            modifier = modifier,
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            if (activeItem.state == "queued") {
                Text(
                    text = "Queued",
                    style = MaterialTheme.typography.labelMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            } else {
                LinearProgressIndicator(
                    progress = { activeItem.progress.coerceIn(0f, 1f) },
                    modifier = Modifier.size(width = 96.dp, height = 4.dp),
                )
            }
            IconButton(onClick = {
                PodcastActionDispatcher.dispatch(
                    bridge = bridge,
                    namespace = PodcastNamespace.PLAYER,
                    payload = CancelDownloadPayload(episodeId = episode.id),
                )
            }) {
                Icon(
                    imageVector = Icons.Filled.Close,
                    contentDescription = "Cancel download",
                    tint = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }

        else -> androidx.compose.material3.OutlinedButton(
            onClick = {
                PodcastActionDispatcher.dispatch(
                    bridge = bridge,
                    namespace = PodcastNamespace.PODCAST,
                    payload = DownloadStartPayload(episodeId = episode.id),
                )
            },
            modifier = modifier,
        ) {
            Icon(
                imageVector = Icons.Filled.Download,
                contentDescription = null,
                modifier = Modifier.size(18.dp),
            )
            Text(text = "  Download")
        }
    }
}

@Composable
private fun DownloadsEmptyState(modifier: Modifier = Modifier) {
    Box(
        modifier = modifier
            .fillMaxSize()
            .padding(24.dp),
        contentAlignment = Alignment.Center,
    ) {
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Icon(
                imageVector = Icons.Filled.Download,
                contentDescription = null,
                modifier = Modifier.size(48.dp),
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Text(
                text = "No downloads yet",
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.Medium,
            )
            Text(
                text = "Episodes you download for offline listening will appear here.",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

/**
 * Build an `episodeId → EpisodeSummary` index from every subscription's
 * episode list so an active-download row (which carries only an id) can be
 * resolved to a title + show name. Recomputed only when the snapshot changes.
 */
@Composable
private fun rememberEpisodeIndex(snapshot: PodcastSnapshot?): Map<String, EpisodeSummary> =
    androidx.compose.runtime.remember(snapshot) {
        val subs = snapshot?.subscriptions ?: return@remember emptyMap()
        val out = LinkedHashMap<String, EpisodeSummary>()
        subs.forEach { sub -> sub.episodes.forEach { ep -> out[ep.id] = ep } }
        out
    }

/** Human-readable status line for an in-flight download row. */
private fun activeStatusLabel(item: DownloadItemSnapshot): String {
    val pct = (item.progress.coerceIn(0f, 1f) * 100f).toInt()
    val size = item.totalBytes?.let { " of ${formatBytes(it)}" } ?: ""
    return when (item.state) {
        "queued" -> "Queued"
        "paused" -> "Paused — $pct%$size"
        else -> "$pct%$size"
    }
}

private fun formatBytes(bytes: Long): String {
    if (bytes <= 0) return "0 B"
    val units = listOf("B", "KB", "MB", "GB")
    var value = bytes.toDouble()
    var unit = 0
    while (value >= 1024.0 && unit < units.lastIndex) {
        value /= 1024.0
        unit++
    }
    return if (unit == 0) "${bytes} B" else "%.1f %s".format(value, units[unit])
}
