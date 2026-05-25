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
import androidx.compose.material3.Card
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.f7z.podcast.PodcastSnapshot
import io.f7z.podcast.PodcastSummary

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
@Composable
fun LibraryScreen(
    snapshot: PodcastSnapshot?,
    onShowSelected: (PodcastSummary) -> Unit,
    modifier: Modifier = Modifier,
) {
    val rows = snapshot?.subscriptions.orEmpty()

    Column(
        modifier = modifier
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
                // M2.A ships `artwork_url`; until Coil is wired, render the
                // placeholder square so layouts don't shift when art arrives.
                ArtworkPlaceholder(size = 120)
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
