package io.f7z.podcast.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Pause
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.FilledIconButton
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.f7z.podcast.EpisodeSummary
import io.f7z.podcast.KernelBridge
import io.f7z.podcast.NowPlayingState
import io.f7z.podcast.PodcastSnapshot

/**
 * Home tab — the first surface the user sees on app launch.
 *
 * Thin-shell composition (D8 — no business logic in the view):
 *
 *  1. "Now Playing" card, rendered iff `snapshot.nowPlaying != null`.
 *     The play/pause button dispatches `podcast.player.play` /
 *     `podcast.player.pause` through [`PodcastActionDispatcher`]; the
 *     kernel decides the resulting state and the next snapshot tick
 *     reflects it.
 *
 *  2. "Recent Episodes" — flattens the first five entries across every
 *     subscribed show. The flatten happens here because the kernel's
 *     M2.A projection emits episodes per-show; a future M2.B+ "recents"
 *     projection will move this into Rust, at which point this Compose
 *     view shrinks to a straight `items(snapshot.recents)`.
 *
 *  3. Empty state — "No recent episodes" when no subscriptions or no
 *     episodes are available yet. Matches the iOS hero's empty render.
 */
@Composable
fun HomeScreen(
    snapshot: PodcastSnapshot?,
    bridge: KernelBridge,
    modifier: Modifier = Modifier,
) {
    LazyColumn(
        modifier = modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
        contentPadding = androidx.compose.foundation.layout.PaddingValues(vertical = 16.dp),
    ) {
        item {
            Text(
                text = "Home",
                style = MaterialTheme.typography.headlineMedium,
                fontWeight = FontWeight.SemiBold,
            )
        }

        val nowPlaying = snapshot?.nowPlaying
        if (nowPlaying != null) {
            item { NowPlayingCard(nowPlaying, bridge) }
        }

        item {
            Text(
                text = "Recent Episodes",
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.SemiBold,
                modifier = Modifier.padding(top = 8.dp),
            )
        }

        val recents = snapshot
            ?.subscriptions
            .orEmpty()
            .flatMap { it.episodes }
            .take(RECENT_EPISODE_LIMIT)

        if (recents.isEmpty()) {
            item { EmptyRecentsCard() }
        } else {
            items(recents, key = { it.id }) { episode ->
                RecentEpisodeCard(episode)
            }
        }
    }
}

@Composable
private fun NowPlayingCard(nowPlaying: NowPlayingState, bridge: KernelBridge) {
    Card(
        colors = CardDefaults.elevatedCardColors(
            containerColor = MaterialTheme.colorScheme.primaryContainer,
        ),
        modifier = Modifier.fillMaxWidth(),
    ) {
        Row(
            modifier = Modifier.padding(16.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            ArtworkPlaceholder(size = 64)
            Column(modifier = Modifier.weight(1f)) {
                Text(
                    text = nowPlaying.episodeTitle ?: "Unknown episode",
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.SemiBold,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
                Spacer(modifier = Modifier.size(2.dp))
                Text(
                    text = nowPlaying.podcastTitle ?: "Unknown show",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onPrimaryContainer,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            PlayPauseIconButton(isPlaying = nowPlaying.isPlaying, bridge = bridge, episodeId = nowPlaying.episodeId)
        }
    }
}

@Composable
private fun PlayPauseIconButton(isPlaying: Boolean, bridge: KernelBridge, episodeId: String?) {
    FilledIconButton(onClick = {
        if (isPlaying) {
            PodcastActionDispatcher.dispatchEmpty(bridge, PodcastActionIds.PLAYER_PAUSE)
        } else if (episodeId != null) {
            PodcastActionDispatcher.dispatch(
                bridge = bridge,
                actionId = PodcastActionIds.PLAYER_PLAY,
                payload = PlayActionPayload(episodeId = episodeId),
            )
        }
    }) {
        Icon(
            imageVector = if (isPlaying) Icons.Filled.Pause else Icons.Filled.PlayArrow,
            contentDescription = if (isPlaying) "Pause" else "Play",
        )
    }
}

@Composable
private fun RecentEpisodeCard(episode: EpisodeSummary) {
    Card(modifier = Modifier.fillMaxWidth()) {
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
                if (!episode.podcastTitle.isNullOrBlank()) {
                    Text(
                        text = episode.podcastTitle,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
        }
    }
}

@Composable
private fun EmptyRecentsCard() {
    Card(modifier = Modifier.fillMaxWidth()) {
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .padding(24.dp),
            contentAlignment = Alignment.Center,
        ) {
            Text(
                text = "No recent episodes",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

/**
 * Stand-in for podcast/episode artwork. The M2.A library projection ships
 * `artwork_url` on every row; a future revision plugs Coil here. Until then,
 * a tinted rounded square keeps the layout stable so we don't reflow when
 * art arrives.
 */
@Composable
internal fun ArtworkPlaceholder(size: Int) {
    Surface(
        modifier = Modifier
            .size(size.dp)
            .clip(RoundedCornerShape(8.dp)),
        color = MaterialTheme.colorScheme.surfaceVariant,
    ) {}
}

private const val RECENT_EPISODE_LIMIT = 5
