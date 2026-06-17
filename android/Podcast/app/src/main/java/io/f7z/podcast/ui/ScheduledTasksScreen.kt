package io.f7z.podcast.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.CalendarMonth
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material3.AssistChip
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SwipeToDismissBox
import androidx.compose.material3.SwipeToDismissBoxValue
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.rememberSwipeToDismissBoxState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.f7z.podcast.AgentTaskSummary
import io.f7z.podcast.KernelBridge
import io.f7z.podcast.PodcastSnapshot
import io.f7z.podcast.TasksActions

/**
 * Scheduled Tasks screen — manage kernel-owned agent tasks.
 *
 * Reactive: task list comes from [PodcastSnapshot.agentTasks] (the
 * `podcast.tasks` domain projection pushed by the kernel). Mutations dispatch
 * via [TasksActions] through [KernelBridge]; no optimistic local state — the
 * kernel re-projects and pushes after each op.
 *
 * Feature parity with iOS `AgentScheduledTasksView`:
 *  - List sorted by nextRunAt (null last), then title.
 *  - Empty state with "Add Task" entry point.
 *  - Per-row: enable/disable switch, run-now button, swipe-to-delete.
 *  - Tap row text area (for `agent_prompt` tasks) opens edit sheet.
 *  - Create/edit form lives in [TaskFormSheet]; dispatches via [TasksActions].
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ScheduledTasksScreen(
    snapshot: PodcastSnapshot?,
    bridge: KernelBridge,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var showCreate by remember { mutableStateOf(false) }
    var editingTask by remember { mutableStateOf<AgentTaskSummary?>(null) }

    // Sort: soonest nextRunAt first; null (no next run / once tasks) go last; ties broken by title.
    val sortedTasks = (snapshot?.agentTasks ?: emptyList())
        .sortedWith(compareBy({ it.nextRunAt ?: Long.MAX_VALUE }, { it.title }))

    Scaffold(
        modifier = modifier,
        topBar = {
            TopAppBar(
                title = { Text("Scheduled Tasks") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
            )
        },
        floatingActionButton = {
            FloatingActionButton(onClick = { showCreate = true }) {
                Icon(Icons.Filled.Add, contentDescription = "New task")
            }
        },
    ) { inner ->
        if (sortedTasks.isEmpty()) {
            TasksEmptyState(onAdd = { showCreate = true }, modifier = Modifier.padding(inner))
        } else {
            LazyColumn(modifier = Modifier.fillMaxSize().padding(inner)) {
                items(sortedTasks, key = { it.id.ifEmpty { it.title } }) { task ->
                    SwipeToDeleteTaskRow(
                        task = task,
                        onToggleEnabled = {
                            if (task.isEnabled) TasksActions.disable(bridge, task.id)
                            else TasksActions.enable(bridge, task.id)
                        },
                        onRunNow = { TasksActions.runNow(bridge, task.id) },
                        onDelete = { TasksActions.delete(bridge, task.id) },
                        onTap = { if (task.intentType == "agent_prompt") editingTask = task },
                    )
                    HorizontalDivider()
                }
            }
        }
    }

    if (showCreate) {
        TaskFormSheet(
            mode = TaskFormMode.Create,
            onDismiss = { showCreate = false },
            onSave = { title, prompt, schedule ->
                TasksActions.createPromptTask(bridge, title, prompt, schedule)
                showCreate = false
            },
        )
    }

    editingTask?.let { task ->
        TaskFormSheet(
            mode = TaskFormMode.Edit(task),
            onDismiss = { editingTask = null },
            onSave = { title, prompt, schedule ->
                TasksActions.updatePromptTask(bridge, task.id, title, prompt, schedule)
                editingTask = null
            },
        )
    }
}

// ── Swipe-to-delete wrapper ───────────────────────────────────────────────────

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun SwipeToDeleteTaskRow(
    task: AgentTaskSummary,
    onToggleEnabled: () -> Unit,
    onRunNow: () -> Unit,
    onDelete: () -> Unit,
    onTap: () -> Unit,
) {
    val dismissState = rememberSwipeToDismissBoxState(
        confirmValueChange = { value ->
            if (value == SwipeToDismissBoxValue.EndToStart) { onDelete(); true } else false
        },
    )
    SwipeToDismissBox(
        state = dismissState,
        enableDismissFromStartToEnd = false,
        backgroundContent = {
            Box(
                modifier = Modifier.fillMaxSize().padding(horizontal = 16.dp),
                contentAlignment = Alignment.CenterEnd,
            ) {
                Icon(Icons.Filled.Delete, "Delete task", tint = MaterialTheme.colorScheme.error)
            }
        },
    ) {
        TaskRowContent(task, onToggleEnabled, onRunNow, onTap)
    }
}

// ── Task row content ──────────────────────────────────────────────────────────

@Composable
private fun TaskRowContent(
    task: AgentTaskSummary,
    onToggleEnabled: () -> Unit,
    onRunNow: () -> Unit,
    onTap: () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(start = 16.dp, end = 4.dp, top = 10.dp, bottom = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Icon(
            Icons.Filled.CalendarMonth,
            contentDescription = null,
            tint = if (task.isEnabled) MaterialTheme.colorScheme.primary
                   else MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Column(
            modifier = Modifier
                .weight(1f)
                .clickable(
                    indication = null,
                    interactionSource = remember { MutableInteractionSource() },
                    onClick = onTap,
                ),
            verticalArrangement = Arrangement.spacedBy(2.dp),
        ) {
            Text(
                text = task.title,
                style = MaterialTheme.typography.bodyLarge,
                fontWeight = FontWeight.Medium,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            val detail = task.intentDetail ?: task.intentLabel.takeIf { it != "Custom task" }
            if (!detail.isNullOrBlank()) {
                Text(
                    text = detail,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            Row(
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                AssistChip(
                    onClick = {},
                    label = {
                        Text(scheduleLabel(task.schedule), style = MaterialTheme.typography.labelSmall)
                    },
                )
                Text(
                    text = nextRunLabel(task),
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                val sLabel = statusLabel(task.status)
                if (sLabel != null) {
                    Text(
                        text = sLabel,
                        style = MaterialTheme.typography.labelSmall,
                        color = statusColor(task.status),
                    )
                }
            }
        }
        IconButton(onClick = onRunNow) {
            Icon(Icons.Filled.PlayArrow, "Run now", tint = MaterialTheme.colorScheme.primary)
        }
        Switch(checked = task.isEnabled, onCheckedChange = { onToggleEnabled() })
    }
}

// ── Empty state ───────────────────────────────────────────────────────────────

@Composable
private fun TasksEmptyState(onAdd: () -> Unit, modifier: Modifier = Modifier) {
    Box(modifier = modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(12.dp),
            modifier = Modifier.padding(horizontal = 32.dp),
        ) {
            Icon(Icons.Filled.CalendarMonth, null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
            Text("No Scheduled Tasks", style = MaterialTheme.typography.titleMedium)
            Text(
                text = "Ask your agent to schedule a recurring task, or tap + to create one.",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                textAlign = TextAlign.Center,
            )
            TextButton(onClick = onAdd) { Text("Add Task") }
        }
    }
}

// ── Display helpers ───────────────────────────────────────────────────────────

private fun scheduleLabel(schedule: String): String = when (schedule) {
    "hourly", "every hour" -> "Hourly"
    "daily", "nightly"     -> "Daily"
    "weekly"               -> "Weekly"
    "once"                 -> "Once"
    else                   -> extractCustomSeconds(schedule)?.let { "Every ${it}s" } ?: schedule
}

private fun nextRunLabel(task: AgentTaskSummary): String {
    if (!task.isEnabled) return "Disabled"
    val next = task.nextRunAt ?: return "No next run"
    val delta = next - System.currentTimeMillis() / 1_000L
    return when {
        delta <= 0     -> "Due now"
        delta < 3_600  -> "Next: ${(delta / 60).coerceAtLeast(1)}m"
        delta < 86_400 -> "Next: ${(delta / 3_600).coerceAtLeast(1)}h"
        else           -> "Next: ${(delta / 86_400).coerceAtLeast(1)}d"
    }
}

private fun statusLabel(status: String): String? = when (status) {
    "failed"    -> "Failed"
    "running"   -> "Running"
    "completed" -> "Completed"
    else        -> null
}

@Composable
private fun statusColor(status: String) = when (status) {
    "failed"    -> MaterialTheme.colorScheme.error
    "running"   -> MaterialTheme.colorScheme.tertiary
    "completed" -> MaterialTheme.colorScheme.primary
    else        -> MaterialTheme.colorScheme.onSurfaceVariant
}
