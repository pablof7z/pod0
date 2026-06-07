package io.f7z.podcast.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material3.Card
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Slider
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import io.f7z.podcast.BriefingSnapshot
import io.f7z.podcast.KernelBridge
import io.f7z.podcast.PodcastSnapshot
import io.f7z.podcast.SettingsSnapshot

/**
 * Settings tab — entry surface for playback, Identity, AI, Briefings, and About.
 *
 * Lean composition: every section is a `Card` containing rows; tapping a
 * navigation row invokes the supplied route callback. No business logic — the briefing schedule label
 * comes straight from `snapshot.briefing.scheduleLabel` (with a sensible
 * default for when M9.A hasn't shipped) and the app version is a literal
 * placeholder until M14 wires `BuildConfig.VERSION_NAME` through here.
 */
@Composable
fun SettingsScreen(
    snapshot: PodcastSnapshot?,
    bridge: KernelBridge,
    onNavigateToIdentity: () -> Unit,
    onNavigateToModels: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val settings = snapshot?.settings ?: SettingsSnapshot()
    LazyColumn(
        modifier = modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
        contentPadding = androidx.compose.foundation.layout.PaddingValues(vertical = 16.dp),
    ) {
        item {
            Text(
                text = "Settings",
                style = MaterialTheme.typography.headlineMedium,
                fontWeight = FontWeight.SemiBold,
            )
        }
        item {
            SettingsSection(title = "Playback") {
                PlaybackSpeedRow(settings = settings, bridge = bridge)
                HorizontalDivider()
                AutoDeleteRow(settings = settings, bridge = bridge)
            }
        }
        item {
            SettingsSection(title = "Identity") {
                SettingsNavRow(label = "Account", onClick = onNavigateToIdentity)
            }
        }
        item {
            SettingsSection(title = "AI") {
                SettingsNavRow(label = "Models", onClick = onNavigateToModels)
            }
        }
        item {
            SettingsSection(title = "Briefings") {
                BriefingsSummaryRow(briefing = snapshot?.briefing)
            }
        }
        item {
            SettingsSection(title = "About") {
                AboutRow()
            }
        }
    }
}

/**
 * Default-playback-rate slider, 0.5×–3.0× in 0.05 steps. Dispatches
 * `podcast.settings` `{"op":"set_default_playback_rate","rate":d}` on release.
 *
 * Holds an ephemeral `dragValue` while the user drags so the thumb doesn't
 * jitter against snapshot ticks; the committed value goes to the kernel and
 * the next snapshot's `default_playback_rate` becomes the source of truth
 * (D5/D8). When not dragging, the displayed value tracks the snapshot.
 */
@Composable
private fun PlaybackSpeedRow(settings: SettingsSnapshot, bridge: KernelBridge) {
    val snapshotRate = settings.defaultPlaybackRate.coerceIn(MIN_RATE, MAX_RATE)
    var dragValue by remember(snapshotRate) { mutableStateOf<Float?>(null) }
    val effective = dragValue ?: snapshotRate

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(text = "Default speed", style = MaterialTheme.typography.bodyLarge)
            Text(
                text = formatRateLabel(effective),
                style = MaterialTheme.typography.bodyLarge,
                fontWeight = FontWeight.Medium,
                color = MaterialTheme.colorScheme.primary,
            )
        }
        Slider(
            value = effective,
            onValueChange = { dragValue = it },
            onValueChangeFinished = {
                dragValue?.let { value ->
                    PodcastActionDispatcher.dispatch(
                        bridge = bridge,
                        namespace = PodcastNamespace.SETTINGS,
                        payload = SetDefaultPlaybackRatePayload(rate = value.toDouble()),
                    )
                }
                dragValue = null
            },
            valueRange = MIN_RATE..MAX_RATE,
            // 0.5..3.0 in 0.05 steps → 50 intervals → 49 internal steps.
            steps = 49,
        )
    }
}

/**
 * Auto-delete-after-played switch. Dispatches `podcast.settings`
 * `{"op":"set_auto_delete_downloads_after_played","enabled":b}`. Reads the
 * checked state straight off the snapshot (no local toggle state).
 */
@Composable
private fun AutoDeleteRow(settings: SettingsSnapshot, bridge: KernelBridge) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        Column(modifier = Modifier.fillMaxWidth(0.78f)) {
            Text(text = "Delete after playing", style = MaterialTheme.typography.bodyLarge)
            Text(
                text = "Remove the downloaded file once an episode is marked played.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Switch(
            checked = settings.autoDeleteDownloads,
            onCheckedChange = { enabled ->
                PodcastActionDispatcher.dispatch(
                    bridge = bridge,
                    namespace = PodcastNamespace.SETTINGS,
                    payload = SetAutoDeleteDownloadsPayload(enabled = enabled),
                )
            },
        )
    }
}

@Composable
private fun SettingsSection(title: String, content: @Composable () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(
            text = title.uppercase(),
            style = MaterialTheme.typography.labelMedium,
            fontWeight = FontWeight.SemiBold,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(start = 4.dp),
        )
        Card(modifier = Modifier.fillMaxWidth()) {
            Column { content() }
        }
    }
}

@Composable
private fun SettingsNavRow(label: String, onClick: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(horizontal = 16.dp, vertical = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            text = label,
            style = MaterialTheme.typography.bodyLarge,
            modifier = Modifier.fillMaxWidth(0.85f),
        )
        Icon(
            imageVector = Icons.AutoMirrored.Filled.KeyboardArrowRight,
            contentDescription = null,
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

@Composable
private fun BriefingsSummaryRow(briefing: BriefingSnapshot?) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 14.dp),
        verticalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        Text(
            text = "Schedule",
            style = MaterialTheme.typography.bodyLarge,
        )
        Text(
            text = briefingScheduleLabel(briefing),
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        val statusLabel = briefing?.status?.takeIf { it.isNotBlank() }
        if (statusLabel != null) {
            HorizontalDivider(modifier = Modifier.padding(vertical = 6.dp))
            Text(
                text = "Status: $statusLabel",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
private fun AboutRow() {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 14.dp),
        verticalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        Text(
            text = "Pod0 (Android)",
            style = MaterialTheme.typography.bodyLarge,
        )
        Text(
            text = "Version $APP_VERSION_PLACEHOLDER",
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

/**
 * Pick a human-readable schedule label. Prefer the snapshot's
 * `schedule_label` (set by M9.A's scheduler), fall back to the conventional
 * "Daily at 7:00 AM" default the iOS surface uses.
 */
private fun briefingScheduleLabel(briefing: BriefingSnapshot?): String {
    briefing?.scheduleLabel?.let { if (it.isNotBlank()) return it }
    val next = briefing?.nextScheduledMinutes
    if (next != null && next > 0) {
        return "Next briefing in $next minutes"
    }
    return DEFAULT_BRIEFING_SCHEDULE
}

private const val DEFAULT_BRIEFING_SCHEDULE = "Daily at 7:00 AM"
private const val APP_VERSION_PLACEHOLDER = "0.1.0"

// Playback-rate bounds match the kernel's server-side clamp `[0.5, 3.0]`
// in `SettingsAction::SetDefaultPlaybackRate`.
private const val MIN_RATE = 0.5f
private const val MAX_RATE = 3.0f

private fun formatRateLabel(rate: Float): String {
    // Snap to the slider's 0.05 grid so the label never shows float noise.
    val snapped = Math.round(rate / 0.05f) * 0.05f
    return if (kotlin.math.abs(snapped - snapped.toInt()) < 0.001f) {
        "${snapped.toInt()}×"
    } else {
        "%.2f".format(snapped).trimEnd('0').trimEnd('.') + "×"
    }
}
