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
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Bookmark
import androidx.compose.material.icons.filled.BookmarkBorder
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.f7z.podcast.BookmarkActions
import io.f7z.podcast.EpisodeSummary
import io.f7z.podcast.KernelBridge
import io.f7z.podcast.PodcastSnapshot
import io.f7z.podcast.PodcastSummary

/**
 * Global Bookmarks screen — every episode across the library where
 * [EpisodeSummary.starred] is `true`, sorted newest-first by `publishedAt`.
 *
 * iOS parity: mirrors `App/Sources/Features/Bookmarks/BookmarksView.swift` which
 * filters `store.episodes` for `episode.isStarred`, sorts by `pubDate` descending,
 * and shows an empty state when none are starred. This screen is the Android
 * equivalent — a pure client-side filter over the existing `snapshot` library
 * rows with no new Rust/FFI surface.
 *
 * Data seam: `snapshot.library` (the `podcast.library` push frame) carries the
 * full episode list including `EpisodeSummary.starred` (`is_starred` in Rust,
 * `starred: Boolean` in [EpisodeSummary]). Verified against:
 *  - `apps/nmp-app-podcast/src/ffi/projections/library.rs` (field: `is_starred`)
 *  - `android/Podcast/…/PodcastSnapshot.kt::EpisodeSummary.starred`
 *
 * Star/unstar action: [BookmarkActions.setStar] dispatches `PodcastAction::StarEpisode`
 * with `starred = false` to the `"podcast"` namespace; the kernel persists the
 * change and the next snapshot tick reflects it reactively in this list.
 *
 * Navigation: reached from Settings via [AppRoute.Bookmarks].
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun BookmarksScreen(
    snapshot: PodcastSnapshot?,
    bridge: KernelBridge,
    onBack: () -> Unit,
    onEpisodeSelected: (EpisodeSummary) -> Unit = {},
    modifier: Modifier = Modifier,
) {
    // Pure client-side filter over the library snapshot.
    // Mirrors iOS BookmarksView.bookmarkedEntries() which checks episode.isStarred.
    val starredEpisodes = starredEpisodesFrom(snapshot)

    Scaffold(
        modifier = modifier,
        topBar = {
            TopAppBar(
                title = { Text("Bookmarks") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(
                            Icons.AutoMirrored.Filled.ArrowBack,
                            contentDescription = "Back",
                        )
                    }
                },
            )
        },
    ) { inner ->
        if (starredEpisodes.isEmpty()) {
            BookmarksEmptyState(modifier = Modifier.padding(inner))
        } else {
            LazyColumn(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(inner),
                verticalArrangement = Arrangement.spacedBy(0.dp),
            ) {
                items(starredEpisodes, key = { it.id }) { episode ->
                    val podcast = snapshot?.subscriptions?.firstOrNull { it.id == episode.podcastId }
                    BookmarkRow(
                        episode = episode,
                        podcast = podcast,
                        onTap = { onEpisodeSelected(episode) },
                        onUnstar = {
                            BookmarkActions.setStar(bridge, episode.id, starred = false)
                        },
                    )
                }
            }
        }
    }
}

// ── Empty state ───────────────────────────────────────────────────────────────

/**
 * Empty state mirroring iOS BookmarksView:
 * "Star an episode, make a clip, or add a note — it will appear here."
 */
@Composable
private fun BookmarksEmptyState(modifier: Modifier = Modifier) {
    Box(
        modifier = modifier.fillMaxSize(),
        contentAlignment = Alignment.Center,
    ) {
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(12.dp),
            modifier = Modifier.padding(horizontal = 32.dp),
        ) {
            Icon(
                imageVector = Icons.Filled.Bookmark,
                contentDescription = null,
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.size(48.dp),
            )
            Text(
                text = "No Bookmarks Yet",
                style = MaterialTheme.typography.titleMedium,
            )
            Text(
                text = "Star an episode to save it here.",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                textAlign = androidx.compose.ui.text.style.TextAlign.Center,
            )
        }
    }
}

// ── Bookmark row ──────────────────────────────────────────────────────────────

@Composable
private fun BookmarkRow(
    episode: EpisodeSummary,
    podcast: PodcastSummary?,
    onTap: () -> Unit,
    onUnstar: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Surface(
        onClick = onTap,
        modifier = modifier.fillMaxWidth(),
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 16.dp, vertical = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(
                modifier = Modifier.weight(1f),
                verticalArrangement = Arrangement.spacedBy(2.dp),
            ) {
                Text(
                    text = episode.title,
                    style = MaterialTheme.typography.bodyLarge,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
                val podcastTitle = podcast?.title
                    ?: episode.podcastTitle
                if (!podcastTitle.isNullOrBlank()) {
                    Text(
                        text = podcastTitle,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                episode.publishedAt?.let { epochSecs ->
                    Text(
                        text = formatBookmarkDate(epochSecs),
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
            Spacer(modifier = Modifier.width(8.dp))
            // Unstar button — tapping removes the episode from this list on the next
            // snapshot tick (kernel owns persistence; Android only dispatches the action).
            IconButton(onClick = onUnstar) {
                Icon(
                    imageVector = Icons.Filled.Bookmark,
                    contentDescription = "Remove bookmark",
                    tint = MaterialTheme.colorScheme.primary,
                )
            }
        }
    }
}

// ── Data helpers ──────────────────────────────────────────────────────────────

/**
 * Filter and sort the starred episodes from [snapshot].
 *
 * Mirrors iOS `BookmarksView.bookmarkedEntries()`:
 *  - includes episodes where `episode.isStarred == true`
 *  - sorts by `pubDate` descending (newest-first)
 *
 * Pure function — no state, no side effects. The snapshot's subscription rows
 * are sourced from the `podcast.library` push frame which carries the full
 * `starred` flag from the Rust projection. Uses [PodcastSnapshot.subscriptions]
 * which prefers the M2.A `podcasts` projection and falls back to `library`.
 */
internal fun starredEpisodesFrom(snapshot: PodcastSnapshot?): List<EpisodeSummary> {
    val subscriptions = snapshot?.subscriptions ?: return emptyList()
    return subscriptions
        .flatMap { it.episodes }
        .filter { it.starred }
        .sortedByDescending { it.publishedAt ?: 0L }
}

// ── Formatters ────────────────────────────────────────────────────────────────

private val DATE_FORMAT = java.text.SimpleDateFormat("MMM d, yyyy", java.util.Locale.getDefault())

private fun formatBookmarkDate(epochSecs: Long): String =
    DATE_FORMAT.format(java.util.Date(epochSecs * 1000L))
