package io.f7z.podcast.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.AddComment
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.AssistChip
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.f7z.podcast.FeedbackReplyDto
import io.f7z.podcast.FeedbackThreadDto
import io.f7z.podcast.KernelBridge
import io.f7z.podcast.PodcastSnapshot

@Composable
fun FeedbackSheet(
    snapshot: PodcastSnapshot?,
    bridge: KernelBridge,
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val threads = snapshot?.feedbackThreads ?: emptyList()
    val signedIn = snapshot?.activeAccount != null
    var composer by remember { mutableStateOf<ComposerTarget?>(null) }
    var expandedThreadId by rememberSaveable { mutableStateOf<String?>(null) }

    LaunchedEffect(bridge) {
        PodcastActionDispatcher.dispatch(
            bridge = bridge,
            namespace = PodcastNamespace.PODCAST,
            payload = FetchFeedbackPayload(),
        )
    }

    Column(
        modifier = modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        FeedbackHeader(
            signedIn = signedIn,
            onRefresh = {
                PodcastActionDispatcher.dispatch(
                    bridge = bridge,
                    namespace = PodcastNamespace.PODCAST,
                    payload = FetchFeedbackPayload(),
                )
            },
            onCompose = { composer = ComposerTarget.Root },
            onDismiss = onDismiss,
        )

        if (!signedIn) {
            Text(
                text = "Sign in from Settings to send feedback.",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }

        if (threads.isEmpty()) {
            EmptyFeedbackState(modifier = Modifier.fillMaxSize().weight(1f))
        } else {
            LazyColumn(
                modifier = Modifier.fillMaxWidth().weight(1f),
                verticalArrangement = Arrangement.spacedBy(10.dp),
                contentPadding = PaddingValues(bottom = 24.dp),
            ) {
                items(threads, key = { it.eventId }) { thread ->
                    FeedbackThreadCard(
                        thread = thread,
                        expanded = expandedThreadId == thread.eventId,
                        signedIn = signedIn,
                        onToggle = {
                            expandedThreadId =
                                if (expandedThreadId == thread.eventId) null else thread.eventId
                        },
                        onReply = {
                            composer = ComposerTarget.Reply(
                                rootEventId = thread.eventId,
                                replyToPubkey = thread.authorPubkey,
                            )
                        },
                    )
                }
            }
        }
    }

    composer?.let { target ->
        FeedbackComposerDialog(
            target = target,
            onDismiss = { composer = null },
            onSubmit = { category, content ->
                PodcastActionDispatcher.dispatch(
                    bridge = bridge,
                    namespace = PodcastNamespace.PODCAST,
                    payload = PublishFeedbackPayload(
                        category = category,
                        content = content,
                        parentEventId = (target as? ComposerTarget.Reply)?.rootEventId,
                        replyToPubkey = (target as? ComposerTarget.Reply)?.replyToPubkey,
                    ),
                )
                composer = null
                PodcastActionDispatcher.dispatch(
                    bridge = bridge,
                    namespace = PodcastNamespace.PODCAST,
                    payload = FetchFeedbackPayload(),
                )
            },
        )
    }
}

@Composable
private fun FeedbackHeader(
    signedIn: Boolean,
    onRefresh: () -> Unit,
    onCompose: () -> Unit,
    onDismiss: () -> Unit,
) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            text = "Feedback",
            style = MaterialTheme.typography.headlineSmall,
            fontWeight = FontWeight.SemiBold,
            modifier = Modifier.weight(1f),
        )
        IconButton(onClick = onRefresh) {
            Icon(imageVector = Icons.Filled.Refresh, contentDescription = "Refresh feedback")
        }
        IconButton(onClick = onCompose, enabled = signedIn) {
            Icon(imageVector = Icons.Filled.AddComment, contentDescription = "New feedback")
        }
        IconButton(onClick = onDismiss) {
            Icon(imageVector = Icons.Filled.Close, contentDescription = "Close feedback")
        }
    }
}

@Composable
private fun EmptyFeedbackState(modifier: Modifier) {
    Box(modifier = modifier, contentAlignment = Alignment.Center) {
        Text(
            text = "No feedback yet",
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

@Composable
private fun FeedbackThreadCard(
    thread: FeedbackThreadDto,
    expanded: Boolean,
    signedIn: Boolean,
    onToggle: () -> Unit,
    onReply: () -> Unit,
) {
    Card(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onToggle),
    ) {
        Column(
            modifier = Modifier.padding(14.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                AssistChip(onClick = {}, label = { Text(categoryLabel(thread.category)) })
                thread.statusLabel?.takeIf { it.isNotBlank() }?.let { status ->
                    AssistChip(onClick = {}, label = { Text(status) })
                }
            }
            Text(
                text = thread.title?.takeIf { it.isNotBlank() } ?: thread.content,
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.SemiBold,
                maxLines = if (expanded) Int.MAX_VALUE else 2,
                overflow = TextOverflow.Ellipsis,
            )
            thread.summary?.takeIf { it.isNotBlank() }?.let { summary ->
                Text(
                    text = summary,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = if (expanded) Int.MAX_VALUE else 2,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            if (expanded) {
                Text(
                    text = thread.content,
                    style = MaterialTheme.typography.bodyMedium,
                )
                thread.replies.forEach { reply -> FeedbackReplyRow(reply = reply) }
                TextButton(onClick = onReply, enabled = signedIn) {
                    Text("Reply")
                }
            } else if (thread.replies.isNotEmpty()) {
                Text(
                    text = "${thread.replies.size} replies",
                    style = MaterialTheme.typography.labelMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}

@Composable
private fun FeedbackReplyRow(reply: FeedbackReplyDto) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(start = 12.dp, top = 6.dp),
        verticalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        Text(
            text = reply.authorPubkey.take(8),
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Text(text = reply.content, style = MaterialTheme.typography.bodyMedium)
    }
}

@Composable
private fun FeedbackComposerDialog(
    target: ComposerTarget,
    onDismiss: () -> Unit,
    onSubmit: (category: String, content: String) -> Unit,
) {
    var category by rememberSaveable { mutableStateOf(FeedbackCategory.Bug) }
    var text by rememberSaveable { mutableStateOf("") }
    val trimmed = text.trim()

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(if (target is ComposerTarget.Reply) "Reply" else "New feedback") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                if (target is ComposerTarget.Root) {
                    FeedbackCategoryPicker(selected = category, onSelected = { category = it })
                }
                OutlinedTextField(
                    value = text,
                    onValueChange = { text = it.take(MAX_FEEDBACK_CHARS) },
                    label = { Text("Message") },
                    minLines = 4,
                    modifier = Modifier.fillMaxWidth(),
                )
                Text(
                    text = "${MAX_FEEDBACK_CHARS - text.length} characters",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        },
        confirmButton = {
            Button(
                enabled = trimmed.isNotBlank(),
                onClick = { onSubmit(category.tag, trimmed) },
            ) {
                Text("Send")
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text("Cancel") }
        },
    )
}

@Composable
private fun FeedbackCategoryPicker(
    selected: FeedbackCategory,
    onSelected: (FeedbackCategory) -> Unit,
) {
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        FeedbackCategory.entries.forEach { category ->
            FilterChip(
                selected = selected == category,
                onClick = { onSelected(category) },
                label = { Text(category.label) },
            )
        }
    }
}

private sealed interface ComposerTarget {
    data object Root : ComposerTarget
    data class Reply(
        val rootEventId: String,
        val replyToPubkey: String,
    ) : ComposerTarget
}

private enum class FeedbackCategory(val tag: String, val label: String) {
    Bug("bug", "Bug"),
    Feature("feature-request", "Feature"),
    Question("question", "Question"),
    Praise("praise", "Praise"),
}

private fun categoryLabel(raw: String): String =
    FeedbackCategory.entries.firstOrNull { it.tag == raw }?.label ?: raw

private const val MAX_FEEDBACK_CHARS = 280
