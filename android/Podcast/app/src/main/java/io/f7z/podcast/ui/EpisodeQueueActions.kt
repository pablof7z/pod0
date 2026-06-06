package io.f7z.podcast.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material3.Icon
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import io.f7z.podcast.KernelBridge

@Composable
fun EpisodeQueueActions(
    episodeId: String,
    isQueued: Boolean,
    bridge: KernelBridge,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier = modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        OutlinedButton(
            onClick = {
                PodcastActionDispatcher.dispatch(
                    bridge = bridge,
                    namespace = PodcastNamespace.QUEUE,
                    payload = QueueAddNextPayload(episodeId = episodeId),
                )
            },
            modifier = Modifier.weight(1f),
        ) {
            Icon(Icons.Filled.PlayArrow, contentDescription = null, modifier = Modifier.size(18.dp))
            Spacer(modifier = Modifier.size(8.dp))
            Text(text = "Play Next")
        }
        OutlinedButton(
            onClick = {
                PodcastActionDispatcher.dispatch(
                    bridge = bridge,
                    namespace = PodcastNamespace.QUEUE,
                    payload = QueueAddLastPayload(episodeId = episodeId),
                )
            },
            modifier = Modifier.weight(1f),
        ) {
            Icon(Icons.Filled.Add, contentDescription = null, modifier = Modifier.size(18.dp))
            Spacer(modifier = Modifier.size(8.dp))
            Text(text = if (isQueued) "Move Last" else "Queue")
        }
    }
}
