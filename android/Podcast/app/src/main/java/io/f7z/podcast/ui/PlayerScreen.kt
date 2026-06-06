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
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Pause
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material3.FilledIconButton
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.f7z.podcast.KernelBridge
import io.f7z.podcast.NowPlayingState
import io.f7z.podcast.PodcastSnapshot

/**
 * Player tab — the now-playing episode detail.
 *
 * Surfaces `snapshot.nowPlaying` with the transport + speed controls. All
 * mutating interactions are kernel-bound:
 *
 *  * Play / pause → namespace `podcast.player`, `{"op":"play",…}` / `{"op":"pause"}`.
 *  * Seek slider → namespace `podcast.player`, `{"op":"seek","position_secs":f64}`.
 *  * Speed chip → namespace `podcast.player`, `{"op":"set_speed","speed":f32}`.
 *
 * The slider holds a tiny ephemeral `dragValue` state while the user is
 * dragging — this is presentation-only (it would otherwise jitter against
 * the snapshot's position updates) and is committed on `onValueChangeFinished`.
 * The committed value is what reaches the kernel; D8 is preserved because
 * the source of truth for "where the playhead is" remains
 * `snapshot.nowPlaying.positionSecs`.
 */
@Composable
fun PlayerScreen(
    snapshot: PodcastSnapshot?,
    bridge: KernelBridge,
    modifier: Modifier = Modifier,
) {
    val nowPlaying = snapshot?.nowPlaying

    Column(
        modifier = modifier
            .fillMaxSize()
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(
            text = "Now Playing",
            style = MaterialTheme.typography.headlineMedium,
            fontWeight = FontWeight.SemiBold,
            modifier = Modifier.fillMaxWidth(),
        )
        if (nowPlaying == null) {
            EmptyPlayerState()
            return@Column
        }
        PlayerHero(nowPlaying)
        SeekBar(nowPlaying = nowPlaying, bridge = bridge)
        TransportRow(nowPlaying = nowPlaying, bridge = bridge)
        SpeedSelector(currentSpeed = nowPlaying.speed, bridge = bridge)
        SleepTimerControl(
            remainingSecs = nowPlaying.sleepTimerRemainingSecs,
            bridge = bridge,
        )
        QueueSection(
            queue = snapshot.queue,
            bridge = bridge,
            modifier = Modifier.fillMaxWidth().weight(1f),
        )
    }
}

@Composable
private fun PlayerHero(nowPlaying: NowPlayingState) {
    Surface(
        modifier = Modifier
            .size(240.dp)
            .clip(RoundedCornerShape(16.dp)),
        color = MaterialTheme.colorScheme.surfaceVariant,
    ) {}
    Spacer(modifier = Modifier.size(4.dp))
    Text(
        text = nowPlaying.episodeTitle ?: "Unknown episode",
        style = MaterialTheme.typography.titleLarge,
        fontWeight = FontWeight.SemiBold,
        textAlign = TextAlign.Center,
        maxLines = 2,
        overflow = TextOverflow.Ellipsis,
        modifier = Modifier.fillMaxWidth(),
    )
    Text(
        text = nowPlaying.podcastTitle ?: "Unknown show",
        style = MaterialTheme.typography.bodyLarge,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        textAlign = TextAlign.Center,
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
        modifier = Modifier.fillMaxWidth(),
    )
}

@Composable
private fun SeekBar(nowPlaying: NowPlayingState, bridge: KernelBridge) {
    // While the user drags, hold the displayed position locally so the slider
    // doesn't jitter against the 4 Hz snapshot tick. Commit to the kernel on
    // release; the next snapshot will catch up. D5/D8 preserved — the kernel
    // still owns the actual playhead.
    var dragValue by remember(nowPlaying.episodeId) { mutableStateOf<Float?>(null) }
    val duration = nowPlaying.durationSecs.toFloat().coerceAtLeast(0f)
    val effectivePosition = dragValue ?: nowPlaying.positionSecs.toFloat().coerceIn(0f, duration)

    Column(modifier = Modifier.fillMaxWidth()) {
        Slider(
            value = effectivePosition,
            onValueChange = { dragValue = it },
            onValueChangeFinished = {
                dragValue?.let { value ->
                    PodcastActionDispatcher.dispatch(
                        bridge = bridge,
                        namespace = PodcastNamespace.PLAYER,
                        payload = SeekPayload(positionSecs = value.toDouble()),
                    )
                }
                dragValue = null
            },
            valueRange = 0f..(if (duration > 0f) duration else 1f),
            enabled = duration > 0f,
        )
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Text(
                text = formatTimecode(effectivePosition.toDouble()),
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Text(
                text = formatTimecode(nowPlaying.durationSecs),
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
private fun TransportRow(nowPlaying: NowPlayingState, bridge: KernelBridge) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.Center,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        FilledIconButton(
            onClick = {
                if (nowPlaying.isPlaying) {
                    PodcastActionDispatcher.dispatch(
                        bridge = bridge,
                        namespace = PodcastNamespace.PLAYER,
                        payload = PausePayload(),
                    )
                } else {
                    val episodeId = nowPlaying.episodeId ?: return@FilledIconButton
                    PodcastActionDispatcher.dispatch(
                        bridge = bridge,
                        namespace = PodcastNamespace.PLAYER,
                        payload = PlayPayload(episodeId = episodeId),
                    )
                }
            },
            modifier = Modifier.size(72.dp),
            colors = IconButtonDefaults.filledIconButtonColors(),
        ) {
            Icon(
                imageVector = if (nowPlaying.isPlaying) Icons.Filled.Pause else Icons.Filled.PlayArrow,
                contentDescription = if (nowPlaying.isPlaying) "Pause" else "Play",
                modifier = Modifier.size(36.dp),
            )
        }
    }
}

@Composable
private fun SpeedSelector(currentSpeed: Float, bridge: KernelBridge) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.CenterHorizontally),
    ) {
        SPEED_OPTIONS.forEach { speed ->
            FilterChip(
                selected = kotlin.math.abs(currentSpeed - speed) < 0.01f,
                onClick = {
                    PodcastActionDispatcher.dispatch(
                        bridge = bridge,
                        namespace = PodcastNamespace.PLAYER,
                        payload = SetSpeedPayload(speed = speed),
                    )
                },
                label = { Text(formatSpeedLabel(speed)) },
            )
        }
    }
}

@Composable
private fun EmptyPlayerState() {
    Box(
        modifier = Modifier
            .fillMaxSize()
            .padding(24.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            text = "Nothing playing yet. Pick an episode from the Library tab.",
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
        )
    }
}

private fun formatTimecode(secs: Double): String {
    if (secs.isNaN() || secs <= 0.0) return "0:00"
    val totalSeconds = secs.toInt()
    val minutes = totalSeconds / 60
    val seconds = totalSeconds % 60
    return "%d:%02d".format(minutes, seconds)
}

private fun formatSpeedLabel(speed: Float): String =
    when {
        kotlin.math.abs(speed - speed.toInt()) < 0.01f -> "${speed.toInt()}×"
        else -> "%.2f×".format(speed).trimEnd('0').trimEnd('.')
    }

private val SPEED_OPTIONS = listOf(0.5f, 1.0f, 1.25f, 1.5f, 2.0f)
