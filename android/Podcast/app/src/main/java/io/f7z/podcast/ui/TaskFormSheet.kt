package io.f7z.podcast.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import io.f7z.podcast.AgentTaskSummary

/**
 * Create/edit form sheet for agent scheduled tasks.
 *
 * Mirrors iOS `AgentScheduledTaskFormSheet`: title + prompt + schedule
 * picker (hourly / daily / weekly / once / custom seconds). Only
 * `agent_prompt` tasks can be created or edited from this sheet — the
 * caller is responsible for only opening it for eligible tasks.
 *
 * `onSave` is called with (title, prompt, schedule) only when all fields
 * are non-blank and the schedule is valid. The sheet does NOT dispatch
 * to the kernel directly — the caller owns the dispatch so this
 * composable stays testable and reusable.
 */

/** Context for whether the sheet is creating a new task or editing an existing one. */
internal sealed interface TaskFormMode {
    data object Create : TaskFormMode
    data class Edit(val task: AgentTaskSummary) : TaskFormMode
}

private val SCHEDULE_OPTIONS = listOf(
    "Hourly" to "hourly",
    "Daily"  to "daily",
    "Weekly" to "weekly",
    "Once"   to "once",
)

@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun TaskFormSheet(
    mode: TaskFormMode,
    onDismiss: () -> Unit,
    onSave: (title: String, prompt: String, schedule: String) -> Unit,
) {
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    var title by remember { mutableStateOf(if (mode is TaskFormMode.Edit) mode.task.title else "") }
    var prompt by remember {
        mutableStateOf(if (mode is TaskFormMode.Edit) mode.task.intentDetail ?: "" else "")
    }
    val seedSched = seedSchedule(mode)
    var selectedSchedule by remember { mutableStateOf(seedSched) }
    var useCustom by remember { mutableStateOf(isCustomSchedule(seedSched)) }
    var customSeconds by remember {
        mutableStateOf(extractCustomSeconds(seedSched)?.toString() ?: "")
    }

    val resolvedSchedule: String? = if (useCustom) {
        customSeconds.trim().toLongOrNull()?.takeIf { it > 0 }?.let { "every ${it}s" }
    } else {
        selectedSchedule
    }
    val saveEnabled = title.isNotBlank() && prompt.isNotBlank() && resolvedSchedule != null

    ModalBottomSheet(onDismissRequest = onDismiss, sheetState = sheetState) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 20.dp)
                .padding(bottom = 32.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    text = if (mode is TaskFormMode.Create) "New Task" else "Edit Task",
                    style = MaterialTheme.typography.titleLarge,
                    fontWeight = FontWeight.SemiBold,
                )
                TextButton(
                    onClick = { resolvedSchedule?.let { onSave(title.trim(), prompt.trim(), it) } },
                    enabled = saveEnabled,
                ) {
                    Text("Save", fontWeight = FontWeight.SemiBold)
                }
            }
            OutlinedTextField(
                value = title,
                onValueChange = { title = it },
                label = { Text("Title") },
                placeholder = { Text("Short name for this task") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            OutlinedTextField(
                value = prompt,
                onValueChange = { prompt = it },
                label = { Text("Prompt") },
                placeholder = { Text("The agent runs this prompt on schedule") },
                minLines = 3,
                maxLines = 6,
                modifier = Modifier.fillMaxWidth(),
            )
            Text("Schedule", style = MaterialTheme.typography.labelLarge)
            SCHEDULE_OPTIONS.forEach { (label, value) ->
                Row(verticalAlignment = Alignment.CenterVertically) {
                    RadioButton(
                        selected = !useCustom && selectedSchedule == value,
                        onClick = { selectedSchedule = value; useCustom = false },
                    )
                    Text(label, modifier = Modifier.clickable {
                        selectedSchedule = value; useCustom = false
                    })
                }
            }
            Row(verticalAlignment = Alignment.CenterVertically) {
                RadioButton(selected = useCustom, onClick = { useCustom = true })
                Text("Custom", modifier = Modifier.clickable { useCustom = true })
            }
            if (useCustom) {
                OutlinedTextField(
                    value = customSeconds,
                    onValueChange = { customSeconds = it },
                    label = { Text("Interval (seconds)") },
                    singleLine = true,
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
                    modifier = Modifier.fillMaxWidth(),
                )
            }
            Spacer(Modifier.height(8.dp))
        }
    }
}

// ── Schedule parse helpers ────────────────────────────────────────────────────

/**
 * Extracts the custom interval in seconds from a schedule string like `"every 3600s"`.
 * Returns null for the standard named schedules or malformed input.
 */
internal fun extractCustomSeconds(schedule: String): Long? {
    if (!schedule.startsWith("every ") || !schedule.endsWith("s")) return null
    return schedule.removePrefix("every ").removeSuffix("s").trim().toLongOrNull()
}

private fun isCustomSchedule(schedule: String): Boolean =
    SCHEDULE_OPTIONS.none { it.second == schedule } && schedule.isNotBlank()

private fun seedSchedule(mode: TaskFormMode): String = when (mode) {
    is TaskFormMode.Create -> "daily"
    is TaskFormMode.Edit   -> mode.task.schedule.ifBlank { "daily" }
}
