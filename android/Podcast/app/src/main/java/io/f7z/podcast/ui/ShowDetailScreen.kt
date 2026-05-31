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
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.Card
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
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
 * Show-detail surface — a single subscribed show + its (windowed) episode
 * list. Reached from the library grid tile tap. The current implementation
 * is a stub per M13.C scope: the title comes from the snapshot's library
 * row, the episode list from the same row's `episodes` field. M2.A's
 * `episodes_for_selected` paged projection will replace the in-row list
 * once it ships.
 *
 * `onBack` is a callback (not a `NavController`) so this composable is
 * usable in previews / tests without the nav graph.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ShowDetailScreen(
    showId: String,
    snapshot: PodcastSnapshot?,
    bridge: KernelBridge,
    onEpisodeSelected: (EpisodeSummary) -> Unit,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
) {
    // Resolve the show by the id carried in the nav route. A library tile
    // passes the Rust subscription UUID; a search-result row passes the
    // iTunes `collectionId` (or feed URL), which never matches a
    // subscription UUID. So fall back to `searchResults` (matching either
    // the result id or its feed URL) before giving up — tapping an
    // unsubscribed result then resolves to its title + the empty-episode
    // state instead of "Show not found".
    val show = snapshot?.subscriptions?.firstOrNull { it.id == showId }
        ?: snapshot?.searchResults?.firstOrNull { it.id == showId || it.feedUrl == showId }
    val activeById = snapshot?.downloads?.active?.associateBy { it.episodeId } ?: emptyMap()

    Scaffold(
        modifier = modifier,
        topBar = {
            TopAppBar(
                title = {
                    Text(
                        text = show?.title ?: "Show",
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
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
    ) { inner ->
        if (show == null) {
            MissingShowState(modifier = Modifier.padding(inner))
            return@Scaffold
        }
        ShowDetailBody(
            episodes = show.episodes,
            activeById = activeById,
            bridge = bridge,
            onEpisodeSelected = onEpisodeSelected,
            modifier = Modifier.padding(inner),
        )
    }
}

@Composable
private fun ShowDetailBody(
    episodes: List<EpisodeSummary>,
    activeById: Map<String, DownloadItemSnapshot>,
    bridge: KernelBridge,
    onEpisodeSelected: (EpisodeSummary) -> Unit,
    modifier: Modifier = Modifier,
) {
    if (episodes.isEmpty()) {
        Box(
            modifier = modifier
                .fillMaxSize()
                .padding(24.dp),
            contentAlignment = Alignment.Center,
        ) {
            Text(
                text = "No episodes yet. The episode list will populate once M2.A's projection ships.",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        return
    }

    LazyColumn(
        modifier = modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
        contentPadding = androidx.compose.foundation.layout.PaddingValues(vertical = 16.dp),
    ) {
        items(episodes, key = { it.id }) { episode ->
            EpisodeRow(
                episode = episode,
                activeItem = activeById[episode.id],
                bridge = bridge,
                onClick = { onEpisodeSelected(episode) },
            )
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun EpisodeRow(
    episode: EpisodeSummary,
    activeItem: DownloadItemSnapshot?,
    bridge: KernelBridge,
    onClick: () -> Unit,
) {
    Card(modifier = Modifier.fillMaxWidth(), onClick = onClick) {
        Row(
            modifier = Modifier.padding(12.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            ArtworkPlaceholder(size = 48)
            Column(modifier = Modifier.weight(1f)) {
                Text(
                    text = episode.title,
                    style = MaterialTheme.typography.titleSmall,
                    fontWeight = FontWeight.Medium,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
                val durationLabel = episode.durationSecs?.let { formatDurationLabel(it) }
                if (!durationLabel.isNullOrBlank()) {
                    Text(
                        text = durationLabel,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
            EpisodeDownloadButton(
                episode = episode,
                activeItem = activeItem,
                bridge = bridge,
            )
        }
    }
}

@Composable
private fun MissingShowState(modifier: Modifier = Modifier) {
    Box(
        modifier = modifier
            .fillMaxSize()
            .padding(24.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            text = "Show not found in current snapshot",
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

private fun formatDurationLabel(durationSecs: Double): String {
    if (durationSecs <= 0) return ""
    val totalMinutes = (durationSecs / 60.0).toInt()
    val hours = totalMinutes / 60
    val minutes = totalMinutes % 60
    return if (hours > 0) "${hours}h ${minutes}m" else "${minutes} min"
}
