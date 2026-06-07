package io.f7z.podcast

import java.util.Locale
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

@Serializable
data class ElevenLabsKeyValidationEnvelope(
    val result: ElevenLabsKeyInfo? = null,
    val error: ElevenLabsKeyValidationError? = null,
)

@Serializable
data class ElevenLabsKeyInfo(
    val tier: String? = null,
    @SerialName("character_count") val characterCount: Long? = null,
    @SerialName("character_limit") val characterLimit: Long? = null,
) {
    val summary: String
        get() {
            val parts = listOfNotNull(
                tier?.takeIf { it.isNotBlank() }?.replaceFirstChar { char ->
                    if (char.isLowerCase()) char.titlecase(Locale.US) else char.toString()
                },
                remainingCharactersLabel(),
            )
            return parts.joinToString(" | ").ifBlank { "ElevenLabs key validated." }
        }

    private fun remainingCharactersLabel(): String? {
        val limit = characterLimit ?: return null
        val count = characterCount ?: return null
        val remaining = (limit - count).coerceAtLeast(0)
        return "${number(remaining)} of ${number(limit)} chars remaining"
    }
}

@Serializable
data class ElevenLabsKeyValidationError(
    val kind: String = "",
    val message: String? = null,
    @SerialName("status_code") val statusCode: Int? = null,
)

class ElevenLabsKeyValidationException(message: String) : Exception(message)

object ElevenLabsKeyValidationService {
    private val json = Json {
        ignoreUnknownKeys = true
        coerceInputValues = true
    }

    suspend fun validateStoredKey(bridge: KernelBridge): ElevenLabsKeyInfo =
        withContext(Dispatchers.IO) {
            val response = bridge.validateElevenLabsKey()
                ?: throw ElevenLabsKeyValidationException("App backend is not ready yet.")
            val envelope = runCatching {
                json.decodeFromString<ElevenLabsKeyValidationEnvelope>(response)
            }.getOrElse {
                throw ElevenLabsKeyValidationException("Unexpected response from ElevenLabs.")
            }
            envelope.error?.let { throw ElevenLabsKeyValidationException(errorMessage(it)) }
            envelope.result ?: throw ElevenLabsKeyValidationException("Unexpected response from ElevenLabs.")
        }

    private fun errorMessage(error: ElevenLabsKeyValidationError): String =
        when (error.kind) {
            "missing_api_key" -> "No stored ElevenLabs key found."
            "invalid_key" -> "Key rejected; check that it is a valid ElevenLabs API key."
            "network_error" -> "Could not reach ElevenLabs. Check your connection."
            "server_error" -> error.statusCode?.let { "ElevenLabs returned HTTP $it." }
                ?: "ElevenLabs returned an error."
            "decoding_error" -> "Unexpected response from ElevenLabs."
            "store_unavailable" -> "App backend is not ready yet."
            else -> error.message ?: "ElevenLabs key could not be validated."
        }
}

private fun number(value: Long): String =
    String.format(Locale.US, "%,d", value)
