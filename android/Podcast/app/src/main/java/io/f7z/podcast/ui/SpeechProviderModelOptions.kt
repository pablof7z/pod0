package io.f7z.podcast.ui

import io.f7z.podcast.SettingsSnapshot
import io.f7z.podcast.STT_ASSEMBLY_AI
import io.f7z.podcast.STT_ELEVEN_LABS_SCRIBE
import io.f7z.podcast.STT_OPENROUTER_WHISPER

internal data class ModelOption(val id: String, val label: String)

internal val ELEVEN_LABS_STT_MODELS = listOf(
    ModelOption("scribe_v1", "Scribe v1"),
)

internal val OPENROUTER_WHISPER_MODELS = listOf(
    ModelOption("openai/whisper-1", "OpenAI Whisper"),
)

internal val ASSEMBLY_AI_STT_MODELS = listOf(
    ModelOption("universal-3-pro,universal-2", "Universal 3 Pro, fallback Universal 2"),
    ModelOption("universal-2", "Universal 2"),
)

internal val ELEVEN_LABS_TTS_MODELS = listOf(
    ModelOption("eleven_turbo_v2_5", "Turbo v2.5"),
    ModelOption("eleven_flash_v2_5", "Flash v2.5"),
    ModelOption("eleven_multilingual_v2", "Multilingual v2"),
)

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
