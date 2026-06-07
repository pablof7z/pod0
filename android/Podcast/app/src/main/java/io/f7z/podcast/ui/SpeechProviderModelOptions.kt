package io.f7z.podcast.ui

import io.f7z.podcast.KernelBridge
import io.f7z.podcast.SettingsSnapshot
import io.f7z.podcast.STT_ASSEMBLY_AI
import io.f7z.podcast.STT_ELEVEN_LABS_SCRIBE
import io.f7z.podcast.STT_OPENROUTER_WHISPER
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

@Serializable
internal data class ModelOption(val id: String, val label: String)

@Serializable
internal data class SpeechModelCatalogEnvelope(
    val result: SpeechModelCatalog? = null,
    val error: String? = null,
)

@Serializable
internal data class SpeechModelCatalog(
    @SerialName("eleven_labs_stt") val elevenLabsStt: List<ModelOption> = emptyList(),
    @SerialName("open_router_whisper") val openRouterWhisper: List<ModelOption> = emptyList(),
    @SerialName("assembly_ai_stt") val assemblyAiStt: List<ModelOption> = emptyList(),
    @SerialName("eleven_labs_tts") val elevenLabsTts: List<ModelOption> = emptyList(),
)

internal object SpeechModelCatalogService {
    private val json = Json {
        ignoreUnknownKeys = true
        coerceInputValues = true
    }

    suspend fun fetchCatalog(bridge: KernelBridge): SpeechModelCatalog =
        withContext(Dispatchers.IO) {
            val response = bridge.speechModelCatalog()
                ?: throw IllegalStateException("Speech model catalog returned null")
            val envelope = json.decodeFromString<SpeechModelCatalogEnvelope>(response)
            envelope.error?.let { throw IllegalStateException(it) }
            envelope.result ?: throw IllegalStateException("Speech model catalog response missing result")
        }
}

internal fun sttStatus(settings: SettingsSnapshot): String {
    val selected = sttDisplayName(settings.sttProvider)
    val effective = sttDisplayName(settings.effectiveSttProvider)
    return if (settings.sttProvider == settings.effectiveSttProvider) {
        "Using $selected"
    } else {
        "Selected $selected; using $effective until the required key is connected."
    }
}

private fun sttDisplayName(provider: String): String = when (provider) {
    STT_ELEVEN_LABS_SCRIBE -> "ElevenLabs Scribe"
    STT_ASSEMBLY_AI -> "AssemblyAI"
    STT_OPENROUTER_WHISPER -> "OpenRouter Whisper"
    else -> "Platform native"
}
