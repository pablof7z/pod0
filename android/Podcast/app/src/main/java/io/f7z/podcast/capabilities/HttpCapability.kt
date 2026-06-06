package io.f7z.podcast.capabilities

import android.util.Base64
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import okhttp3.MediaType.Companion.toMediaTypeOrNull
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import java.util.concurrent.TimeUnit

/**
 * Android executor for `nmp.http.capability`.
 *
 * The kernel chooses the URL, method, headers, and body. This class performs
 * that HTTP request with OkHttp and reports the raw result as data.
 */
class HttpCapability {
    private val client = OkHttpClient.Builder()
        .connectTimeout(TIMEOUT_SECONDS, TimeUnit.SECONDS)
        .readTimeout(TIMEOUT_SECONDS, TimeUnit.SECONDS)
        .writeTimeout(TIMEOUT_SECONDS, TimeUnit.SECONDS)
        .retryOnConnectionFailure(true)
        .build()

    @Volatile
    private var started = false

    fun start() {
        started = true
    }

    fun stop() {
        started = false
    }

    fun handle(request: CapabilityRequest): String {
        val result = process(request)
        return CapabilityWire.envelope(
            namespace = NAMESPACE,
            correlationId = request.correlationId,
            resultJson = result.encode(),
        )
    }

    private fun process(request: CapabilityRequest): HttpResult {
        if (!started) return HttpResult.Error("capability-stopped")
        val payload = runCatching {
            CapabilityWire.json.decodeFromString(HttpRequestPayload.serializer(), request.payloadJson)
        }.getOrNull() ?: return HttpResult.Error("malformed-payload")

        val builder = Request.Builder().url(payload.url)
        for (pair in payload.headers) {
            if (pair.size == 2) builder.header(pair[0], pair[1])
        }

        val bodyBytes = when {
            payload.bodyBase64 != null -> runCatching {
                Base64.decode(payload.bodyBase64, Base64.DEFAULT)
            }.getOrNull() ?: return HttpResult.Error("invalid-body-base64")
            payload.body != null -> payload.body.toByteArray(Charsets.UTF_8)
            else -> null
        }
        val body = bodyBytes?.toRequestBody("application/octet-stream".toMediaTypeOrNull())
        val okhttpRequest = when (payload.method.uppercase()) {
            "GET" -> builder.get().build()
            "POST" -> builder.post(body ?: ByteArray(0).toRequestBody()).build()
            else -> return HttpResult.Error("unsupported-method")
        }

        return runCatching {
            client.newCall(okhttpRequest).execute().use { response ->
                val headers = buildList {
                    for (i in 0 until response.headers.size) {
                        add(listOf(response.headers.name(i), response.headers.value(i)))
                    }
                }
                HttpResult.Ok(
                    statusCode = response.code,
                    headers = headers,
                    body = response.body?.string().orEmpty(),
                )
            }
        }.getOrElse { error ->
            HttpResult.Error(error.message ?: "transport-error")
        }
    }

    companion object {
        const val NAMESPACE = "nmp.http.capability"
        private const val TIMEOUT_SECONDS = 20L
    }
}

@Serializable
private data class HttpRequestPayload(
    val method: String,
    val url: String,
    val headers: List<List<String>> = emptyList(),
    val body: String? = null,
    @SerialName("body_base64") val bodyBase64: String? = null,
)

private sealed interface HttpResult {
    fun encode(): String

    data class Ok(
        val statusCode: Int,
        val headers: List<List<String>> = emptyList(),
        val body: String,
    ) : HttpResult {
        override fun encode(): String = encodeJson(buildJsonObject {
            put("status", JsonPrimitive("ok"))
            put("status_code", JsonPrimitive(statusCode))
            if (headers.isNotEmpty()) {
                put("headers", buildJsonArray {
                    headers.forEach { pair ->
                        add(buildJsonArray {
                            add(JsonPrimitive(pair.getOrElse(0) { "" }))
                            add(JsonPrimitive(pair.getOrElse(1) { "" }))
                        })
                    }
                })
            }
            put("body", JsonPrimitive(body))
        })
    }

    data class Error(val message: String) : HttpResult {
        override fun encode(): String = encodeJson(buildJsonObject {
            put("status", JsonPrimitive("error"))
            put("message", JsonPrimitive(message))
        })
    }

    companion object {
        private fun encodeJson(obj: JsonObject): String =
            CapabilityWire.json.encodeToString(JsonObject.serializer(), obj)
    }
}
