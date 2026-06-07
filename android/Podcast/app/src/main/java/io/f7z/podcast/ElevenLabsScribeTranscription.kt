package io.f7z.podcast

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

@Serializable
data class ElevenLabsScribeIntent(
    @SerialName("audio_url") val audioUrl: String,
    @SerialName("language_hint") val languageHint: String? = null,
)

@Serializable
data class ElevenLabsScribeEnvelope(
    val result: ElevenLabsScribeResult? = null,
    val error: ElevenLabsScribeBackendError? = null,
)

@Serializable
data class ElevenLabsScribeBackendError(
    val kind: String,
    val message: String? = null,
    @SerialName("status_code") val statusCode: Int? = null,
)

@Serializable
data class ElevenLabsScribeResult(
    @SerialName("language_code") val languageCode: String? = null,
    val text: String? = null,
    val words: List<ElevenLabsScribeWord> = emptyList(),
    val model: String = "",
    val duration: Double? = null,
    @SerialName("latency_ms") val latencyMs: Long = 0,
)

@Serializable
data class ElevenLabsScribeWord(
    val text: String,
    val start: Double,
    val end: Double,
    @SerialName("type") val type: String? = null,
    @SerialName("speaker_id") val speakerId: String? = null,
)

object ElevenLabsScribeTranscriptionService {
    private val json = Json {
        ignoreUnknownKeys = true
        coerceInputValues = true
    }

    suspend fun transcribe(
        bridge: KernelBridge,
        intent: ElevenLabsScribeIntent,
    ): ElevenLabsScribeResult = withContext(Dispatchers.IO) {
        val request = json.encodeToString(ElevenLabsScribeIntent.serializer(), intent)
        val response = bridge.elevenLabsScribeTranscribe(request)
            ?: throw IllegalStateException("ElevenLabs Scribe returned null")
        val envelope = json.decodeFromString(ElevenLabsScribeEnvelope.serializer(), response)
        envelope.error?.let { error ->
            val suffix = error.statusCode?.let { " (HTTP $it)" }.orEmpty()
            throw IllegalStateException((error.message ?: error.kind) + suffix)
        }
        envelope.result ?: throw IllegalStateException("ElevenLabs Scribe response missing result")
    }
}
