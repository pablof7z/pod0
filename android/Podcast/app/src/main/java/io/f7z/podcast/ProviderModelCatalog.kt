package io.f7z.podcast

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

@Serializable
data class ProviderModelCatalogEnvelope(
    val result: ProviderModelCatalogResult? = null,
    val error: String? = null,
)

@Serializable
data class ProviderModelCatalogResult(
    val models: List<ProviderModelOption> = emptyList(),
)

@Serializable
data class ProviderModelOption(
    val provider: String = "",
    val id: String = "",
    val name: String = "",
    @SerialName("provider_id") val providerId: String = "",
    @SerialName("provider_name") val providerName: String = "",
    @SerialName("provider_icon_url") val providerIconUrl: String? = null,
    @SerialName("model_description") val modelDescription: String? = null,
    @SerialName("prompt_cost_per_million") val promptCostPerMillion: Double? = null,
    @SerialName("completion_cost_per_million") val completionCostPerMillion: Double? = null,
    @SerialName("cache_read_cost_per_million") val cacheReadCostPerMillion: Double? = null,
    @SerialName("cache_write_cost_per_million") val cacheWriteCostPerMillion: Double? = null,
    @SerialName("request_cost") val requestCost: Double? = null,
    @SerialName("image_cost") val imageCost: Double? = null,
    @SerialName("web_search_cost") val webSearchCost: Double? = null,
    @SerialName("context_length") val contextLength: Long? = null,
    @SerialName("output_limit") val outputLimit: Long? = null,
    @SerialName("input_modalities") val inputModalities: List<String> = emptyList(),
    @SerialName("output_modalities") val outputModalities: List<String> = emptyList(),
    val tokenizer: String? = null,
    @SerialName("supports_tools") val supportsTools: Boolean = false,
    @SerialName("supports_reasoning") val supportsReasoning: Boolean = false,
    @SerialName("supports_response_format") val supportsResponseFormat: Boolean = false,
    @SerialName("supports_structured_outputs") val supportsStructuredOutputs: Boolean = false,
    @SerialName("open_weights") val openWeights: Boolean = false,
    @SerialName("is_moderated") val isModerated: Boolean? = null,
    @SerialName("created_at_epoch_secs") val createdAtEpochSecs: Long? = null,
    @SerialName("knowledge_cutoff") val knowledgeCutoff: String? = null,
    @SerialName("release_date") val releaseDate: String? = null,
    @SerialName("last_updated") val lastUpdated: String? = null,
    @SerialName("search_text") val searchText: String = "",
) {
    val displayName: String
        get() = name.ifBlank { id }

    val isCompatible: Boolean
        get() {
            val textOutput = outputModalities.isEmpty() || outputModalities.contains("text")
            return textOutput && supportsResponseFormat
        }

    val isFree: Boolean
        get() = promptCostPerMillion == 0.0 && completionCostPerMillion == 0.0

    val compactPricing: String
        get() {
            if (isFree) return "Free"
            val prompt = promptCostPerMillion
            val completion = completionCostPerMillion
            if (prompt == null && completion == null) return "Usage"
            return "${'$'}${formatCost(prompt)}/${'$'}${formatCost(completion)}"
        }

    val contextLabel: String?
        get() = contextLength?.let(::formatTokenCount)

    fun matches(query: String): Boolean {
        val terms = query.lowercase().split(Regex("\\s+")).filter { it.isNotBlank() }
        if (terms.isEmpty()) return true
        val haystack = searchText.ifBlank {
            listOf(
                id,
                name,
                provider,
                providerId,
                providerName,
                modelDescription.orEmpty(),
                tokenizer.orEmpty(),
                inputModalities.joinToString(" "),
                outputModalities.joinToString(" "),
            ).joinToString(" ").lowercase()
        }
        return terms.all { haystack.contains(it) }
    }
}

object ProviderModelCatalogService {
    private val json = Json {
        ignoreUnknownKeys = true
        coerceInputValues = true
    }

    suspend fun fetchModels(bridge: KernelBridge): List<ProviderModelOption> =
        withContext(Dispatchers.IO) {
            val response = bridge.providerModelCatalog()
                ?: throw IllegalStateException("Provider catalog returned null")
            val envelope = json.decodeFromString<ProviderModelCatalogEnvelope>(response)
            envelope.error?.let { throw IllegalStateException(it) }
            envelope.result?.models ?: throw IllegalStateException("Provider catalog response missing result")
        }
}

private fun formatCost(value: Double?): String {
    val cost = value ?: return "-"
    return when {
        cost == 0.0 -> "0"
        cost < 0.01 -> "%.4f".format(cost).trimEnd('0').trimEnd('.')
        cost < 1.0 -> "%.2f".format(cost).trimEnd('0').trimEnd('.')
        else -> "%.1f".format(cost).trimEnd('0').trimEnd('.')
    }
}

private fun formatTokenCount(value: Long): String =
    when {
        value >= 1_000_000 -> "${value / 1_000_000}M ctx"
        value >= 1_000 -> "${value / 1_000}K ctx"
        else -> "$value ctx"
    }
