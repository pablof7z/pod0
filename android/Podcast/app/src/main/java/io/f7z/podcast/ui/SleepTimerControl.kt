package io.f7z.podcast.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.windowInsetsBottomHeight
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Bedtime
import androidx.compose.material3.AssistChip
import androidx.compose.material3.AssistChipDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import io.f7z.podcast.KernelBridge

/**
 * Sleep-timer affordance + preset picker.
 *
 * The countdown comes straight from `nowPlaying.sleepTimerRemainingSecs`;
 * Android only renders it and forwards preset/cancel actions to NMP.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun SleepTimerControl(remainingSecs: Long?, bridge: KernelBridge) {
    var showSheet by remember { mutableStateOf(false) }
    val active = remainingSecs != null && remainingSecs > 0

    AssistChip(
        onClick = { showSheet = true },
        leadingIcon = {
            Icon(
                imageVector = Icons.Filled.Bedtime,
                contentDescription = null,
                modifier = Modifier.size(18.dp),
            )
        },
        label = {
            Text(
                text = if (active) "Sleep in ${formatCountdown(remainingSecs!!)}" else "Sleep timer",
            )
        },
        colors = if (active) {
            AssistChipDefaults.assistChipColors(
                labelColor = MaterialTheme.colorScheme.primary,
                leadingIconContentColor = MaterialTheme.colorScheme.primary,
            )
        } else {
            AssistChipDefaults.assistChipColors()
        },
    )

    if (showSheet) {
        val sheetState = rememberModalBottomSheetState()
        ModalBottomSheet(
            onDismissRequest = { showSheet = false },
            sheetState = sheetState,
        ) {
            Text(
                text = "Sleep Timer",
                style = MaterialTheme.typography.titleLarge,
                fontWeight = FontWeight.SemiBold,
                modifier = Modifier.padding(horizontal = 24.dp, vertical = 8.dp),
            )
            SLEEP_TIMER_PRESETS.forEach { minutes ->
                ListItem(
                    headlineContent = { Text("$minutes minutes") },
                    modifier = Modifier.fillMaxWidth().clickable {
                        PodcastActionDispatcher.dispatch(
                            bridge = bridge,
                            namespace = PodcastNamespace.PLAYER,
                            payload = SleepTimerPayload(secs = minutes * 60),
                        )
                        showSheet = false
                    },
                )
            }
            if (active) {
                HorizontalDivider()
                ListItem(
                    headlineContent = {
                        Text(
                            text = "Cancel timer",
                            color = MaterialTheme.colorScheme.error,
                        )
                    },
                    modifier = Modifier.fillMaxWidth().clickable {
                        PodcastActionDispatcher.dispatch(
                            bridge = bridge,
                            namespace = PodcastNamespace.PLAYER,
                            payload = SleepTimerPayload(secs = null),
                        )
                        showSheet = false
                    },
                )
            }
            Spacer(
                modifier = Modifier.windowInsetsBottomHeight(WindowInsets.navigationBars),
            )
        }
    }
}

private val SLEEP_TIMER_PRESETS = listOf(15, 30, 45, 60)

private fun formatCountdown(secs: Long): String {
    val clamped = secs.coerceAtLeast(0L)
    val minutes = clamped / 60
    val seconds = clamped % 60
    return "%d:%02d".format(minutes, seconds)
}
