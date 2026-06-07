package io.f7z.podcast

import java.util.Locale
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

@Serializable
data class OpenRouterKeyValidationEnvelope(
    val result: OpenRouterKeyInfo? = null,
    val error: OpenRouterKeyValidationError? = null,
)

@Serializable
data class OpenRouterKeyInfo(
    val label: String? = null,
    @SerialName("usage_dollars") val usageDollars: Double? = null,
    @SerialName("limit_dollars") val limitDollars: Double? = null,
    @SerialName("is_free_tier") val isFreeTier: Boolean = false,
    @SerialName("requests_per_interval") val requestsPerInterval: Int? = null,
    @SerialName("rate_interval") val rateInterval: String? = null,
) {
    val summary: String
        get() {
            val parts = listOfNotNull(
                label?.takeIf { it.isNotBlank() },
                remainingCreditLabel(),
                rateLimitLabel(),
                if (isFreeTier) "Free tier" else null,
            )
            return parts.joinToString(" | ").ifBlank { "OpenRouter key validated." }
        }

    private fun remainingCreditLabel(): String? {
        val limit = limitDollars ?: return null
        val usage = usageDollars ?: return null
        val remaining = (limit - usage).coerceAtLeast(0.0)
        return "${money(remaining)} remaining of ${money(limit)}"
    }

    private fun rateLimitLabel(): String? {
        val requests = requestsPerInterval ?: return null
        val interval = rateInterval?.takeIf { it.isNotBlank() } ?: return null
        return "$requests requests/$interval"
    }
}

@Serializable
data class OpenRouterKeyValidationError(
    val kind: String = "",
    val message: String? = null,
    @SerialName("status_code") val statusCode: Int? = null,
)

class OpenRouterKeyValidationException(message: String) : Exception(message)

object OpenRouterKeyValidationService {
    private val json = Json {
        ignoreUnknownKeys = true
        coerceInputValues = true
    }

    suspend fun validateStoredKey(bridge: KernelBridge): OpenRouterKeyInfo =
        withContext(Dispatchers.IO) {
            val response = bridge.validateOpenRouterKey()
                ?: throw OpenRouterKeyValidationException("App backend is not ready yet.")
            val envelope = runCatching {
                json.decodeFromString<OpenRouterKeyValidationEnvelope>(response)
            }.getOrElse {
                throw OpenRouterKeyValidationException("Unexpected response from OpenRouter.")
            }
            envelope.error?.let { throw OpenRouterKeyValidationException(errorMessage(it)) }
            envelope.result ?: throw OpenRouterKeyValidationException("Unexpected response from OpenRouter.")
        }

    private fun errorMessage(error: OpenRouterKeyValidationError): String =
        when (error.kind) {
            "missing_api_key" -> "No stored OpenRouter key found."
            "invalid_key" -> "Key rejected; check that it is a valid OpenRouter API key."
            "network_error" -> "Could not reach OpenRouter. Check your connection."
            "server_error" -> error.statusCode?.let { "OpenRouter returned HTTP $it." }
                ?: "OpenRouter returned an error."
            "decoding_error" -> "Unexpected response from OpenRouter."
            "store_unavailable" -> "App backend is not ready yet."
            else -> error.message ?: "OpenRouter key could not be validated."
        }
}

private fun money(value: Double): String =
    String.format(Locale.US, "$" + "%.4f", value).trimEnd('0').trimEnd('.')
