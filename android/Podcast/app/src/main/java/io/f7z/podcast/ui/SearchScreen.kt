package io.f7z.podcast.ui

import androidx.compose.foundation.clickable
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
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import coil.compose.AsyncImage
import io.f7z.podcast.KernelBridge
import io.f7z.podcast.PodcastSnapshot
import io.f7z.podcast.PodcastSummary
import kotlinx.coroutines.delay

/**
 * Search tab — find any podcast via the kernel's iTunes/RSS directory search.
 *
 * Flow (all kernel-bound, D5/D8 — the kernel owns every result + the
 * subscription set; this screen only renders and dispatches):
 *
 *  1. The query field debounces 500 ms, then dispatches
 *     `{"op":"search_itunes","query":…}` to the `podcast` namespace.
 *  2. Results arrive on `snapshot.searchResults` (a `List<PodcastSummary>`,
 *     each row carrying `feedUrl` + `artworkUrl` + `author`).
 *  3. The subscribe button normalizes valid HTTP(S) feed URLs, then dispatches
 *     `{"op":"subscribe","feed_url":…}`. The screen then watches
 *     `snapshot.subscriptions` for a row whose normalized `feedUrl` matches
 *     the one it subscribed; when it appears, [onSubscribed] navigates
 *     to the show-detail surface.
 *  4. Tapping a result row (anywhere but the button) calls [onResultTapped]
 *     for the host to push a detail surface.
 *
 * `pendingFeedUrl` is the only local state with behavioral weight: it drives
 * the per-row loading spinner and the "navigate when it lands" effect. It's a
 * UI-coordination latch, not a cache of kernel state.
 */
@Composable
fun SearchScreen(
    snapshot: PodcastSnapshot?,
    bridge: KernelBridge,
    onSubscribed: (showId: String) -> Unit,
    onResultTapped: (PodcastSummary) -> Unit,
    modifier: Modifier = Modifier,
) {
    var query by remember { mutableStateOf("") }
    var pendingFeedUrl by remember { mutableStateOf<String?>(null) }

    // Debounce: re-dispatch search 500 ms after the last keystroke. `query`
    // is the effect key, so a new keystroke cancels the in-flight delay.
    LaunchedEffect(query) {
        val trimmed = query.trim()
        if (trimmed.isEmpty()) return@LaunchedEffect
        delay(500)
        PodcastActionDispatcher.dispatch(
            bridge = bridge,
            namespace = PodcastNamespace.PODCAST,
            payload = SearchPayload(query = trimmed),
        )
    }

    // Once a subscribe lands (the feed appears in the library projection),
    // navigate to its detail surface and clear the pending latch.
    val subscriptions = snapshot?.subscriptions.orEmpty()
    LaunchedEffect(pendingFeedUrl, subscriptions) {
        val feed = pendingFeedUrl ?: return@LaunchedEffect
        subscriptions.firstOrNull {
            FeedUrlNormalizer.normalizedFeedUrl(it.feedUrl) == feed
        }?.let { show ->
            pendingFeedUrl = null
            onSubscribed(show.id)
        }
    }

    val results = snapshot?.searchResults.orEmpty()

    Column(
        modifier = modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp, vertical = 16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        OutlinedTextField(
            value = query,
            onValueChange = { query = it },
            label = { Text("Search podcasts") },
            leadingIcon = { Icon(Icons.Filled.Search, contentDescription = null) },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )
        when {
            results.isNotEmpty() -> SearchResultList(
                results = results,
                pendingFeedUrl = pendingFeedUrl,
                onSubscribe = { show ->
                    val feed = FeedUrlNormalizer.normalizedFeedUrl(show.feedUrl)
                        ?: return@SearchResultList
                    pendingFeedUrl = feed
                    PodcastActionDispatcher.dispatch(
                        bridge = bridge,
                        namespace = PodcastNamespace.PODCAST,
                        payload = SubscribePayload(feedUrl = feed),
                    )
                },
                onResultTapped = onResultTapped,
            )
            query.isBlank() -> SearchHint("Search for any podcast by name.")
            else -> SearchHint("No results yet for \"${query.trim()}\".")
        }
    }
}

@Composable
private fun SearchResultList(
    results: List<PodcastSummary>,
    pendingFeedUrl: String?,
    onSubscribe: (PodcastSummary) -> Unit,
    onResultTapped: (PodcastSummary) -> Unit,
) {
    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        items(results, key = { it.id }) { show ->
            val normalizedFeedUrl = FeedUrlNormalizer.normalizedFeedUrl(show.feedUrl)
            SearchResultRow(
                show = show,
                isSubscribing = pendingFeedUrl != null && pendingFeedUrl == normalizedFeedUrl,
                canSubscribe = normalizedFeedUrl != null,
                onSubscribe = { onSubscribe(show) },
                onTap = { onResultTapped(show) },
            )
        }
    }
}

@Composable
private fun SearchResultRow(
    show: PodcastSummary,
    isSubscribing: Boolean,
    canSubscribe: Boolean,
    onSubscribe: () -> Unit,
    onTap: () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onTap)
            .padding(vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        ResultArtwork(url = show.artworkUrl)
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = show.title,
                style = MaterialTheme.typography.titleSmall,
                fontWeight = FontWeight.Medium,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
            val author = show.author
            if (!author.isNullOrBlank()) {
                Text(
                    text = author,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        Button(
            onClick = onSubscribe,
            enabled = !isSubscribing && canSubscribe,
        ) {
            if (isSubscribing) {
                CircularProgressIndicator(
                    modifier = Modifier.size(18.dp),
                    strokeWidth = 2.dp,
                    color = MaterialTheme.colorScheme.onPrimary,
                )
            } else {
                Text("Subscribe")
            }
        }
    }
}

@Composable
private fun ResultArtwork(url: String?) {
    val shape = RoundedCornerShape(8.dp)
    if (url.isNullOrBlank()) {
        Box(
            modifier = Modifier
                .size(56.dp)
                .clip(shape)
                .padding(0.dp),
        ) { ArtworkPlaceholder(size = 56) }
    } else {
        AsyncImage(
            model = url,
            contentDescription = null,
            modifier = Modifier
                .size(56.dp)
                .clip(shape),
        )
    }
}

@Composable
private fun SearchHint(text: String) {
    Box(
        modifier = Modifier
            .fillMaxSize()
            .padding(24.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            text = text,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}
