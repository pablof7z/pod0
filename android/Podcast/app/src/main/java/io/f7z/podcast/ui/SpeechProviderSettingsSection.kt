package io.f7z.podcast.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Card
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import io.f7z.podcast.KernelBridge
import io.f7z.podcast.SettingsSnapshot
import io.f7z.podcast.STT_APPLE_NATIVE
import io.f7z.podcast.STT_ASSEMBLY_AI
import io.f7z.podcast.STT_ELEVEN_LABS_SCRIBE
import io.f7z.podcast.STT_OPENROUTER_WHISPER

@Composable
internal fun SpeechProviderSettingsSection(
    settings: SettingsSnapshot,
    bridge: KernelBridge,
    speechCatalog: SpeechModelCatalog,
    catalogError: String?,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(
            text = "SPEECH",
            style = MaterialTheme.typography.labelMedium,
            fontWeight = FontWeight.SemiBold,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(start = 4.dp),
        )
        Card(modifier = Modifier.fillMaxWidth()) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Text(
                    text = "Speech-to-text provider",
                    style = MaterialTheme.typography.bodyLarge,
                    fontWeight = FontWeight.Medium,
                )
                Text(
                    text = sttStatus(settings),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                if (catalogError != null) {
                    Text(
                        text = catalogError,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.error,
                    )
                }
                SttProviderButton(
                    title = "Platform native",
                    detail = "On-device transcription when the host supports it; no key required.",
                    provider = STT_APPLE_NATIVE,
                    selectedProvider = settings.sttProvider,
                    keyPresent = true,
                    bridge = bridge,
                )
                SttProviderButton(
                    title = "ElevenLabs Scribe",
                    detail = "Cloud speech-to-text using the ElevenLabs key.",
                    provider = STT_ELEVEN_LABS_SCRIBE,
                    selectedProvider = settings.sttProvider,
                    keyPresent = settings.elevenLabsKeyPresent,
                    bridge = bridge,
                )
                SttProviderButton(
                    title = "AssemblyAI",
                    detail = "Cloud transcription with speaker labels and word timestamps.",
                    provider = STT_ASSEMBLY_AI,
                    selectedProvider = settings.sttProvider,
                    keyPresent = settings.assemblyAiKeyPresent,
                    bridge = bridge,
                )
                SttProviderButton(
                    title = "OpenRouter Whisper",
                    detail = "Whisper transcription through the shared OpenRouter transport.",
                    provider = STT_OPENROUTER_WHISPER,
                    selectedProvider = settings.sttProvider,
                    keyPresent = settings.openRouterKeyPresent,
                    bridge = bridge,
                )

                HorizontalDivider()

                SpeechModelChooser(
                    title = "ElevenLabs STT model",
                    current = settings.elevenLabsSttModel,
                    options = speechCatalog.elevenLabsStt,
                    onSelect = { model ->
                        PodcastActionDispatcher.dispatch(
                            bridge = bridge,
                            namespace = PodcastNamespace.SETTINGS,
                            payload = SetElevenLabsModelsPayload(
                                sttModel = model,
                                ttsModel = settings.elevenLabsTtsModel,
                            ),
                        )
                    },
                )
                SpeechModelChooser(
                    title = "OpenRouter Whisper model",
                    current = settings.openRouterWhisperModel,
                    options = speechCatalog.openRouterWhisper,
                    onSelect = { model ->
                        PodcastActionDispatcher.dispatch(
                            bridge = bridge,
                            namespace = PodcastNamespace.SETTINGS,
                            payload = SetOpenRouterWhisperModelPayload(model = model),
                        )
                    },
                )
                SpeechModelChooser(
                    title = "AssemblyAI STT model",
                    current = settings.assemblyAiSttModel,
                    options = speechCatalog.assemblyAiStt,
                    onSelect = { model ->
                        PodcastActionDispatcher.dispatch(
                            bridge = bridge,
                            namespace = PodcastNamespace.SETTINGS,
                            payload = SetAssemblyAiSttModelPayload(model = model),
                        )
                    },
                )

                HorizontalDivider()

                SpeechModelChooser(
                    title = "ElevenLabs TTS model",
                    current = settings.elevenLabsTtsModel,
                    options = speechCatalog.elevenLabsTts,
                    onSelect = { model ->
                        PodcastActionDispatcher.dispatch(
                            bridge = bridge,
                            namespace = PodcastNamespace.SETTINGS,
                            payload = SetElevenLabsModelsPayload(
                                sttModel = settings.elevenLabsSttModel,
                                ttsModel = model,
                            ),
                        )
                    },
                )
                ElevenLabsVoiceEditor(settings = settings, bridge = bridge)
            }
        }
    }
}
