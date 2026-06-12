package io.f7z.podcast.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import io.f7z.podcast.AgentMessageSummary
import io.f7z.podcast.AgentSnapshot
import io.f7z.podcast.KernelBridge
import io.f7z.podcast.PodcastSnapshot

/**
 * Agent Chat — a thin-shell Compose surface for the kernel's built-in agent.
 *
 * ## Architecture (D5/D7/D8)
 *
 * All state lives in the kernel; this screen only renders the
 * `snapshot.agent` projection and dispatches ops through the existing
 * `KernelBridge.dispatchAction` / [PodcastActionDispatcher] path:
 *
 *  - **Send** → `{"op":"send","message":"…"}` on `podcast.agent`
 *    ([AgentSendPayload])
 *  - **Clear** → `{"op":"clear"}` on `podcast.agent`
 *    ([AgentClearPayload])
 *
 * ## Transcript lifecycle
 *
 * The kernel echoes the user turn into `AgentSnapshot.messages` immediately
 * so it appears before the LLM replies. While `isBusy == true` the
 * send button is disabled and a [LinearProgressIndicator] runs below the
 * app-bar. When `isGenerating == true` on the last assistant message a
 * typing-indicator bubble is shown inline. The final reply arrives via the
 * reactive push frame — no polling.
 *
 * ## Wire ops (source of truth: agent_module.rs)
 *
 * ```
 * AgentChatAction::Send  { message: String }  →  op = "send"
 * AgentChatAction::Clear                       →  op = "clear"
 * ```
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AgentChatScreen(
    snapshot: PodcastSnapshot?,
    bridge: KernelBridge,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val agent: AgentSnapshot? = snapshot?.agent
    val messages = agent?.messages ?: emptyList()
    val isBusy = agent?.isBusy ?: false

    var draft by remember { mutableStateOf("") }

    // Scroll to bottom whenever the transcript grows.
    val listState = rememberLazyListState()
    LaunchedEffect(messages.size) {
        if (messages.isNotEmpty()) {
            listState.animateScrollToItem(messages.lastIndex)
        }
    }

    Scaffold(
        modifier = modifier,
        topBar = {
            TopAppBar(
                title = { Text("Agent") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
                actions = {
                    IconButton(
                        onClick = {
                            PodcastActionDispatcher.dispatch(
                                bridge = bridge,
                                namespace = PodcastNamespace.AGENT,
                                payload = AgentClearPayload(),
                            )
                        },
                    ) {
                        Icon(Icons.Filled.Delete, contentDescription = "Clear conversation")
                    }
                },
            )
        },
        bottomBar = {
            AgentChatComposer(
                draft = draft,
                isBusy = isBusy,
                onDraftChange = { draft = it },
                onSend = {
                    val text = draft.trim()
                    if (text.isNotEmpty()) {
                        PodcastActionDispatcher.dispatch(
                            bridge = bridge,
                            namespace = PodcastNamespace.AGENT,
                            payload = AgentSendPayload(message = text),
                        )
                        draft = ""
                    }
                },
            )
        },
    ) { innerPadding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(innerPadding),
        ) {
            if (isBusy) {
                LinearProgressIndicator(modifier = Modifier.fillMaxWidth())
            }
            if (messages.isEmpty()) {
                AgentChatEmptyState(modifier = Modifier.weight(1f))
            } else {
                LazyColumn(
                    state = listState,
                    modifier = Modifier
                        .weight(1f)
                        .padding(horizontal = 12.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                    contentPadding = androidx.compose.foundation.layout.PaddingValues(vertical = 12.dp),
                ) {
                    items(messages, key = { it.id }) { msg ->
                        AgentMessageBubble(msg = msg)
                    }
                }
            }
        }
    }
}

// ── Composer ──────────────────────────────────────────────────────────────────

@Composable
private fun AgentChatComposer(
    draft: String,
    isBusy: Boolean,
    onDraftChange: (String) -> Unit,
    onSend: () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .imePadding()
            .padding(horizontal = 12.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        OutlinedTextField(
            value = draft,
            onValueChange = onDraftChange,
            placeholder = { Text("Ask the agent…") },
            modifier = Modifier.weight(1f),
            singleLine = false,
            maxLines = 4,
            enabled = !isBusy,
        )
        if (isBusy) {
            CircularProgressIndicator(modifier = Modifier.size(36.dp), strokeWidth = 3.dp)
        } else {
            IconButton(
                onClick = onSend,
                enabled = draft.isNotBlank(),
            ) {
                Icon(
                    Icons.AutoMirrored.Filled.Send,
                    contentDescription = "Send",
                    tint = if (draft.isNotBlank())
                        MaterialTheme.colorScheme.primary
                    else
                        MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}

// ── Message bubble ────────────────────────────────────────────────────────────

@Composable
private fun AgentMessageBubble(msg: AgentMessageSummary) {
    val isUser = msg.role == "user"
    Box(
        modifier = Modifier.fillMaxWidth(),
        contentAlignment = if (isUser) Alignment.CenterEnd else Alignment.CenterStart,
    ) {
        Card(
            shape = RoundedCornerShape(
                topStart = 12.dp,
                topEnd = 12.dp,
                bottomStart = if (isUser) 12.dp else 4.dp,
                bottomEnd = if (isUser) 4.dp else 12.dp,
            ),
            colors = CardDefaults.cardColors(
                containerColor = if (isUser)
                    MaterialTheme.colorScheme.primaryContainer
                else
                    MaterialTheme.colorScheme.surfaceVariant,
            ),
            modifier = Modifier.padding(
                start = if (isUser) 48.dp else 0.dp,
                end = if (isUser) 0.dp else 48.dp,
            ),
        ) {
            Column(modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp)) {
                if (msg.isGenerating && msg.content.isBlank()) {
                    TypingIndicator()
                } else {
                    Text(
                        text = msg.content,
                        style = MaterialTheme.typography.bodyMedium,
                        color = if (isUser)
                            MaterialTheme.colorScheme.onPrimaryContainer
                        else
                            MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        }
    }
}

@Composable
private fun TypingIndicator() {
    Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        Text("•", style = MaterialTheme.typography.bodyMedium)
        Text("•", style = MaterialTheme.typography.bodyMedium)
        Text("•", style = MaterialTheme.typography.bodyMedium)
    }
}

// ── Empty state ───────────────────────────────────────────────────────────────

@Composable
private fun AgentChatEmptyState(modifier: Modifier = Modifier) {
    Box(modifier = modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(8.dp),
            modifier = Modifier.padding(32.dp),
        ) {
            Text(
                text = "Chat with your podcast agent",
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.SemiBold,
            )
            Text(
                text = "Ask about shows, get recommendations, or search your library.",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}
