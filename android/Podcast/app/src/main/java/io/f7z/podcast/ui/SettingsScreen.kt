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
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import io.f7z.podcast.BriefingSnapshot
import io.f7z.podcast.PodcastSnapshot

/**
 * Settings tab — entry surface for Identity, Briefings, and About.
 *
 * Lean composition: every section is a `Card` containing rows; tapping a
 * navigation row invokes [`onNavigateToIdentity`] which the host routes
 * into a stack push. No business logic — the briefing schedule label
 * comes straight from `snapshot.briefing.scheduleLabel` (with a sensible
 * default for when M9.A hasn't shipped) and the app version is a literal
 * placeholder until M14 wires `BuildConfig.VERSION_NAME` through here.
 */
@Composable
fun SettingsScreen(
    snapshot: PodcastSnapshot?,
    onNavigateToIdentity: () -> Unit,
    modifier: Modifier = Modifier,
) {
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
            SettingsSection(title = "Identity") {
                SettingsNavRow(label = "Account", onClick = onNavigateToIdentity)
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
