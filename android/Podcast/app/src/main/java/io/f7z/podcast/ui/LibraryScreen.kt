package io.f7z.podcast.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.material.ExperimentalMaterialApi
import androidx.compose.material.pullrefresh.PullRefreshIndicator
import androidx.compose.material.pullrefresh.pullRefresh
import androidx.compose.material.pullrefresh.rememberPullRefreshState
import androidx.compose.material3.Card
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import coil.compose.AsyncImage
import io.f7z.podcast.KernelBridge
import io.f7z.podcast.PodcastSnapshot
import io.f7z.podcast.PodcastSummary
import kotlinx.coroutines.delay

/**
 * Library tab — a two-column grid of subscribed shows.
 *
 * Binds directly to `snapshot.subscriptions` — the merged getter on
 * [`PodcastSnapshot`] that prefers M2.A's `podcasts` projection and falls
 * back to the legacy `library` wire field when M2.A hasn't shipped yet.
 * Tapping a tile invokes
 * [`onShowSelected`] which the host navigator translates into a route
 * change to [`ShowDetailScreen`] — kept as a callback so this composable
 * is testable without a `NavController`.
 *
 * D8/D5 — no derived state. The kernel is the source of truth for
 * subscription rows; this view only renders.
 */
@OptIn(ExperimentalMaterialApi::class)
@Composable
fun LibraryScreen(
    snapshot: PodcastSnapshot?,
    bridge: KernelBridge,
    onShowSelected: (PodcastSummary) -> Unit,
    modifier: Modifier = Modifier,
) {
    val rows = snapshot?.subscriptions.orEmpty()

    // The kernel refresh is fire-and-forget — there is no "refresh complete"
    // signal on the snapshot (new episodes simply arrive on later ticks). So
    // the spinner is shown for a fixed beat after dispatch, then dismissed;
    // the grid updates reactively as the projection lands.
    var refreshing by remember { mutableStateOf(false) }
    LaunchedEffect(refreshing) {
        if (refreshing) {
            delay(1200)
            refreshing = false
        }
    }
    val pullState = rememberPullRefreshState(
        refreshing = refreshing,
        onRefresh = {
            refreshing = true
            PodcastActionDispatcher.dispatch(
                bridge = bridge,
                namespace = PodcastNamespace.PODCAST,
                payload = RefreshAllPayload(),
            )
        },
    )

    Box(modifier = modifier.fillMaxSize().pullRefresh(pullState)) {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(horizontal = 16.dp, vertical = 16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            Text(
                text = "Library",
                style = MaterialTheme.typography.headlineMedium,
                fontWeight = FontWeight.SemiBold,
            )
            if (rows.isEmpty()) {
                EmptyLibraryState()
            } else {
                LibraryGrid(rows = rows, onShowSelected = onShowSelected)
            }
        }
        PullRefreshIndicator(
            refreshing = refreshing,
            state = pullState,
            modifier = Modifier.align(Alignment.TopCenter),
        )
    }
}

@Composable
private fun LibraryGrid(rows: List<PodcastSummary>, onShowSelected: (PodcastSummary) -> Unit) {
    LazyVerticalGrid(
        columns = GridCells.Fixed(2),
        verticalArrangement = Arrangement.spacedBy(12.dp),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        modifier = Modifier.fillMaxSize(),
    ) {
        items(rows, key = { it.id }) { show ->
            LibraryTile(show = show, onClick = { onShowSelected(show) })
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun LibraryTile(show: PodcastSummary, onClick: () -> Unit) {
    Card(
        modifier = Modifier
            .fillMaxWidth(),
        onClick = onClick,
    ) {
        Column(modifier = Modifier.padding(8.dp)) {
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .aspectRatio(1f),
                contentAlignment = Alignment.Center,
            ) {
                val art = show.artworkUrl
                if (art.isNullOrBlank()) {
                    ArtworkPlaceholder(size = 120)
                } else {
                    AsyncImage(
                        model = art,
                        contentDescription = null,
                        modifier = Modifier.fillMaxWidth().aspectRatio(1f),
                    )
                }
            }
            Text(
                text = show.title,
                style = MaterialTheme.typography.titleSmall,
                fontWeight = FontWeight.Medium,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.padding(top = 8.dp),
            )
            Text(
                text = "${show.episodeCount} ep • ${show.unplayedCount} unplayed",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

@Composable
private fun EmptyLibraryState() {
    Box(
        modifier = Modifier
            .fillMaxSize()
            .padding(24.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            text = "No subscriptions yet. Library will populate once M2.A's projection ships.",
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}
