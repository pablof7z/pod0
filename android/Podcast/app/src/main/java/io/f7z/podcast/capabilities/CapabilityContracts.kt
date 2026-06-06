package io.f7z.podcast.capabilities

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject

@Serializable
data class CapabilityRequest(
    val namespace: String = "",
    @SerialName("correlation_id") val correlationId: String = "",
    @SerialName("payload_json") val payloadJson: String = "",
)

@Serializable
data class CapabilityEnvelope(
    val namespace: String,
    @SerialName("correlation_id") val correlationId: String,
    @SerialName("result_json") val resultJson: String,
)

internal object CapabilityWire {
    val json: Json = Json { encodeDefaults = true; ignoreUnknownKeys = true }

    fun ok(namespace: String, correlationId: String): String =
        envelope(namespace, correlationId, resultJson("ok"))

    fun error(namespace: String, correlationId: String, message: String): String =
        envelope(
            namespace = namespace,
            correlationId = correlationId,
            resultJson = resultJson("error", message),
        )

    fun envelope(namespace: String, correlationId: String, resultJson: String): String =
        json.encodeToString(
            CapabilityEnvelope(
                namespace = namespace,
                correlationId = correlationId,
                resultJson = resultJson,
            ),
        )

    private fun resultJson(status: String, message: String? = null): String =
        json.encodeToString(
            JsonObject.serializer(),
            buildJsonObject {
                put("status", JsonPrimitive(status))
                if (message != null) put("message", JsonPrimitive(message))
            },
        )
}
