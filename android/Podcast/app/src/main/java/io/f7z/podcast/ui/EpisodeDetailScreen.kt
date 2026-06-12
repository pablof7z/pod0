package io.f7z.podcast.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.Subtitles
import androidx.compose.material3.AssistChip
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.core.text.HtmlCompat
import coil.compose.AsyncImage
import io.f7z.podcast.ChapterSummary
import io.f7z.podcast.EpisodeSummary
import io.f7z.podcast.KernelBridge
import io.f7z.podcast.PodcastSnapshot
import io.f7z.podcast.TranscriptEntry
import java.text.DateFormat
import java.util.Date

/** Episode metadata resolved from the live Rust snapshot; all actions dispatch back to NMP. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun EpisodeDetailScreen(
    episodeId: String,
    podcastId: String,
    snapshot: PodcastSnapshot?,
    bridge: KernelBridge,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val show = snapshot?.subscriptions?.firstOrNull { it.id == podcastId }
    val episode = show?.episodes?.firstOrNull { it.id == episodeId }
        ?: snapshot?.subscriptions
            ?.flatMap { it.episodes }
            ?.firstOrNull { it.id == episodeId }

    Scaffold(
        modifier = modifier,
        topBar = {
            TopAppBar(
                title = {
                    Text(
                        text = episode?.title ?: "Episode",
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
            )
        },
    ) { inner ->
        if (episode == null) {
            MissingEpisodeState(modifier = Modifier.padding(inner))
            return@Scaffold
        }
        val activeItem = snapshot?.downloads?.active?.firstOrNull { it.episodeId == episode.id }
        EpisodeDetailBody(
            episode = episode,
            podcastTitle = episode.podcastTitle ?: show?.title,
            artworkUrl = episode.artworkUrl ?: show?.artworkUrl,
            activeItem = activeItem,
            isQueued = snapshot?.queue?.any { it.id == episode.id } == true,
            bridge = bridge,
            onPlay = {
                PodcastActionDispatcher.dispatch(
                    bridge = bridge,
                    namespace = PodcastNamespace.PLAYER,
                    payload = PlayPayload(episodeId = episode.id),
                )
            },
            modifier = Modifier.padding(inner),
        )
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun EpisodeDetailBody(
    episode: EpisodeSummary,
    podcastTitle: String?,
    artworkUrl: String?,
    activeItem: io.f7z.podcast.DownloadItemSnapshot?,
    isQueued: Boolean,
    bridge: KernelBridge,
    onPlay: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Text(
            text = episode.title,
            style = MaterialTheme.typography.headlineSmall,
            fontWeight = FontWeight.SemiBold,
        )
        ShowRow(podcastTitle = podcastTitle, artworkUrl = artworkUrl)

        Button(onClick = onPlay, modifier = Modifier.fillMaxWidth()) {
            Icon(Icons.Filled.PlayArrow, contentDescription = null, modifier = Modifier.size(20.dp))
            Text(text = "  Play", style = MaterialTheme.typography.titleMedium)
        }

        EpisodeQueueActions(
            episodeId = episode.id,
            isQueued = isQueued,
            bridge = bridge,
            modifier = Modifier.fillMaxWidth(),
        )

        EpisodeDownloadButton(
            episode = episode,
            activeItem = activeItem,
            bridge = bridge,
            modifier = Modifier.fillMaxWidth(),
        )

        MetadataLine(episode = episode)
        ResumeBar(episode = episode)
        AiCategoryChips(categories = episode.aiCategories)

        val notes = episode.description?.let { stripHtml(it) }
        if (!notes.isNullOrBlank()) {
            Text(text = "Show notes", style = MaterialTheme.typography.titleSmall, fontWeight = FontWeight.Medium)
            Text(text = notes, style = MaterialTheme.typography.bodyMedium)
        }

        ChapterSection(episode = episode, bridge = bridge)
        TranscriptSection(episode = episode, bridge = bridge)
    }
}

@Composable
private fun ShowRow(podcastTitle: String?, artworkUrl: String?) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        val shape = RoundedCornerShape(8.dp)
        if (artworkUrl.isNullOrBlank()) {
            Box(modifier = Modifier.size(56.dp).clip(shape)) { ArtworkPlaceholder(size = 56) }
        } else {
            AsyncImage(
                model = artworkUrl,
                contentDescription = null,
                modifier = Modifier.size(56.dp).clip(shape),
            )
        }
        Text(
            text = podcastTitle ?: "Unknown show",
            style = MaterialTheme.typography.titleMedium,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

@Composable
private fun MetadataLine(episode: EpisodeSummary) {
    val parts = buildList {
        episode.durationSecs?.let { secs -> formatDuration(secs).takeIf { it.isNotBlank() }?.let(::add) }
        episode.publishedAt?.let { add(formatDate(it)) }
    }
    if (parts.isEmpty()) return
    Text(
        text = parts.joinToString("  •  "),
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
}

@Composable
private fun ResumeBar(episode: EpisodeSummary) {
    val position = episode.playbackPositionSecs ?: return
    val duration = episode.durationSecs ?: return
    if (position <= 0.0 || duration <= 0.0) return
    val fraction = (position / duration).coerceIn(0.0, 1.0).toFloat()
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        LinearProgressIndicator(progress = { fraction }, modifier = Modifier.fillMaxWidth())
        Text(
            text = "Resume at ${formatTimecodeShort(position)}",
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun AiCategoryChips(categories: List<String>) {
    if (categories.isEmpty()) return
    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        categories.forEach { category ->
            AssistChip(onClick = {}, label = { Text(category) })
        }
    }
}

/**
 * Chapter section: renders existing chapters when present; otherwise shows a
 * "Generate Chapters" affordance when the episode has a transcript available.
 *
 * D7: the kernel decides whether chapters exist or synthesis is viable —
 * we surface the action only when there is a transcript to ground it and no
 * chapters yet (matching the iOS `AIChapterCompiler` guard).
 *
 * Dispatches `podcast.chapters` `{"op":"compile","episode_id":"..."}` —
 * verified against `ChaptersAction::Compile { episode_id: String }`.
 */
@Composable
private fun ChapterSection(episode: EpisodeSummary, bridge: KernelBridge) {
    val hasChapters = episode.chapters.isNotEmpty()
    val hasTranscript = !episode.transcript.isNullOrBlank() || episode.transcriptEntries.isNotEmpty()

    if (!hasChapters && !hasTranscript) return

    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(
            text = "Chapters",
            style = MaterialTheme.typography.titleSmall,
            fontWeight = FontWeight.Medium,
        )
        if (hasChapters) {
            episode.chapters.forEach { chapter ->
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    Text(
                        text = formatTimecodeShort(chapter.startSecs),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    Text(
                        text = chapter.title.ifBlank { "Untitled chapter" },
                        style = MaterialTheme.typography.bodySmall,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
        } else {
            OutlinedButton(
                onClick = {
                    PodcastActionDispatcher.dispatch(
                        bridge = bridge,
                        namespace = PodcastNamespace.CHAPTERS,
                        payload = CompileChaptersPayload(episodeId = episode.id),
                    )
                },
                modifier = Modifier.fillMaxWidth(),
            ) {
                Text("Generate Chapters")
            }
        }
    }
}

/**
 * Transcript section for the episode detail view.
 *
 * Lifecycle rendering:
 *  - No `transcript_url` and no entries → hidden (nothing to offer).
 *  - `transcript_url` present but no transcript yet → "Load Transcript" button
 *    which dispatches `podcast` `{"op":"fetch_transcript","episode_id":"..."}`.
 *  - `transcript_status` non-empty → in-progress shimmer or error label.
 *  - `transcript_entries` non-empty → timestamped segment list (collapsible).
 *  - `transcript` (raw text) present but no entries → plain text fallback.
 *
 * D8 — all state derives from the kernel snapshot; no local business logic.
 */
@Composable
private fun TranscriptSection(episode: EpisodeSummary, bridge: KernelBridge) {
    val hasUrl = !episode.transcriptUrl.isNullOrBlank()
    val hasEntries = episode.transcriptEntries.isNotEmpty()
    val hasText = !episode.transcript.isNullOrBlank()
    val status = episode.transcriptStatus
    val statusMsg = episode.transcriptStatusMessage

    if (!hasUrl && !hasEntries && !hasText && status.isBlank()) return

    var expanded by rememberSaveable { mutableStateOf(false) }

    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                Icon(
                    imageVector = Icons.Filled.Subtitles,
                    contentDescription = null,
                    modifier = Modifier.size(18.dp),
                    tint = MaterialTheme.colorScheme.primary,
                )
                Text(
                    text = "Transcript",
                    style = MaterialTheme.typography.titleSmall,
                    fontWeight = FontWeight.Medium,
                )
            }
            if (hasEntries || hasText) {
                IconButton(onClick = { expanded = !expanded }, modifier = Modifier.size(32.dp)) {
                    Text(
                        text = if (expanded) "Hide" else "Show",
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.primary,
                    )
                }
            }
        }

        when {
            status == "fetching_publisher" || status == "transcribing" || status == "queued" -> {
                LinearProgressIndicator(modifier = Modifier.fillMaxWidth())
                Text(
                    text = when (status) {
                        "queued" -> "Transcript queued"
                        "fetching_publisher" -> "Fetching transcript…"
                        else -> "Transcribing…"
                    },
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            status == "failed" -> {
                Text(
                    text = statusMsg ?: "Transcript failed",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.error,
                )
                TranscriptFetchButton(episodeId = episode.id, bridge = bridge)
            }
            hasEntries && expanded -> TranscriptEntryList(entries = episode.transcriptEntries)
            hasText && expanded -> {
                Text(
                    text = episode.transcript!!,
                    style = MaterialTheme.typography.bodySmall,
                )
            }
            !hasEntries && !hasText && hasUrl -> {
                TranscriptFetchButton(episodeId = episode.id, bridge = bridge)
            }
        }
    }
}

@Composable
private fun TranscriptFetchButton(episodeId: String, bridge: KernelBridge) {
    OutlinedButton(
        onClick = {
            PodcastActionDispatcher.dispatch(
                bridge = bridge,
                namespace = PodcastNamespace.PODCAST,
                payload = FetchTranscriptPayload(episodeId = episodeId),
            )
        },
        modifier = Modifier.fillMaxWidth(),
    ) {
        Text("Load Transcript")
    }
}

@Composable
private fun TranscriptEntryList(entries: List<TranscriptEntry>) {
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        entries.take(TRANSCRIPT_PREVIEW_LIMIT).forEach { entry ->
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Text(
                    text = formatTimecodeShort(entry.startSecs),
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(top = 1.dp),
                )
                Column {
                    entry.speaker?.takeIf { it.isNotBlank() }?.let { speaker ->
                        Text(
                            text = speaker,
                            style = MaterialTheme.typography.labelSmall,
                            fontWeight = FontWeight.Medium,
                            color = MaterialTheme.colorScheme.primary,
                        )
                    }
                    Text(text = entry.text, style = MaterialTheme.typography.bodySmall)
                }
            }
        }
        if (entries.size > TRANSCRIPT_PREVIEW_LIMIT) {
            Text(
                text = "… ${entries.size - TRANSCRIPT_PREVIEW_LIMIT} more segments",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

private const val TRANSCRIPT_PREVIEW_LIMIT = 50

@Composable
private fun MissingEpisodeState(modifier: Modifier = Modifier) {
    Box(
        modifier = modifier.fillMaxSize().padding(24.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            text = "Episode not found in current snapshot",
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

private fun stripHtml(raw: String): String =
    HtmlCompat.fromHtml(raw, HtmlCompat.FROM_HTML_MODE_LEGACY).toString().trim()

private fun formatDuration(durationSecs: Double): String {
    if (durationSecs <= 0) return ""
    val totalMinutes = (durationSecs / 60.0).toInt()
    val hours = totalMinutes / 60
    val minutes = totalMinutes % 60
    return if (hours > 0) "${hours}h ${minutes}m" else "$minutes min"
}

private fun formatTimecodeShort(secs: Double): String {
    if (secs.isNaN() || secs < 0) return "0:00"
    val total = secs.toInt()
    val h = total / 3600
    val m = (total % 3600) / 60
    val s = total % 60
    return if (h > 0) "%d:%02d:%02d".format(h, m, s) else "%d:%02d".format(m, s)
}

private fun formatDate(unixSeconds: Long): String =
    DateFormat.getDateInstance(DateFormat.MEDIUM).format(Date(unixSeconds * 1000L))
