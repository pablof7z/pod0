package io.f7z.podcast.capabilities

import android.util.Base64
import io.f7z.podcast.KernelBridge
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
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
 * Android executor for `nmp.http.capability` (synchronous) and
 * `nmp.http.async.capability` (fire-and-forget).
 *
 * The kernel chooses the URL, method, headers, and body. This class performs
 * that HTTP request with OkHttp and reports the raw result as data.
 *
 * The **synchronous** namespace blocks the calling capability-socket thread
 * and returns the result inside the envelope (used by iTunes/transcript/etc.).
 * The **async** namespace (optimistic subscribe) runs the request on
 * [`scope`] (`Dispatchers.IO`), returns an immediate ack envelope so the kernel
 * actor thread is never blocked, and posts the result back out of band via
 * [`KernelBridge.httpReport`] — mirroring how [`DownloadCapability`] reports
 * through [`KernelBridge.downloadReport`].
 */
class HttpCapability(
    private val bridge: KernelBridge,
) {
    private val client = OkHttpClient.Builder()
        .connectTimeout(TIMEOUT_SECONDS, TimeUnit.SECONDS)
        .readTimeout(TIMEOUT_SECONDS, TimeUnit.SECONDS)
        .writeTimeout(TIMEOUT_SECONDS, TimeUnit.SECONDS)
        .retryOnConnectionFailure(true)
        .build()

    /**
     * Async requests are scoped to a `SupervisorJob` on `Dispatchers.IO` so the
     * actor thread never blocks on the RSS fetch. The `KernelBridge.httpReport`
     * call is guarded by `handle != 0L`, so a report that lands after
     * `bridge.free()` is a no-op.
     */
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

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

    /**
     * Fire-and-forget entry point for `nmp.http.async.capability`. Decodes the
     * `HttpCommand`, launches the transport on [`scope`], and returns an
     * immediate ack envelope. The kernel drops this ack (exactly like
     * `dispatch_download`); the real result arrives via `bridge.httpReport`.
     * A malformed command still acks (the pending kernel fetch simply isn't
     * resolved — a later refresh retries), never throws (D6).
     */
    fun handleAsync(request: CapabilityRequest): String {
        val command = runCatching {
            CapabilityWire.json.decodeFromString(HttpCommandPayload.serializer(), request.payloadJson)
        }.getOrNull()
        if (command != null) {
            executeAsync(command)
        }
        return CapabilityWire.ok(ASYNC_NAMESPACE, request.correlationId)
    }

    private fun executeAsync(command: HttpCommandPayload) {
        scope.launch {
            val result = if (!started) {
                HttpResult.Error("capability-stopped")
            } else {
                perform(command.request)
            }
            val report = buildJsonObject {
                put("request_id", JsonPrimitive(command.requestId))
                put("result", result.toJson())
            }
            val reportJson = CapabilityWire.json.encodeToString(JsonObject.serializer(), report)
            runCatching { bridge.httpReport(reportJson) }
        }
    }

    private fun process(request: CapabilityRequest): HttpResult {
        if (!started) return HttpResult.Error("capability-stopped")
        val payload = runCatching {
            CapabilityWire.json.decodeFromString(HttpRequestPayload.serializer(), request.payloadJson)
        }.getOrNull() ?: return HttpResult.Error("malformed-payload")
        return perform(payload)
    }

    /**
     * Build and execute the request with the shared OkHttp [`client`]. Reused by
     * the synchronous and async paths so the transport logic lives once. Returns
     * the result as data — a transport failure is `HttpResult.Error`, never an
     * exception (D6).
     */
    private fun perform(payload: HttpRequestPayload): HttpResult {
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
        const val ASYNC_NAMESPACE = "nmp.http.async.capability"
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

/**
 * Async `HttpCommand` payload — mirrors the Rust `HttpCommand`
 * (`{ "request_id": "…", "request": { … } }`).
 */
@Serializable
private data class HttpCommandPayload(
    @SerialName("request_id") val requestId: String,
    val request: HttpRequestPayload,
)

private sealed interface HttpResult {
    /**
     * The result as a JSON object. The async report nests this directly under
     * `HttpReport.result`, so it MUST be a `JsonObject` — a stringified
     * `encode()` would round-trip through Rust serde as a quoted string and the
     * `HttpReport` decode would fail. `encode()` is the stringified form for the
     * synchronous envelope's `result_json`.
     */
    fun toJson(): JsonObject

    fun encode(): String

    data class Ok(
        val statusCode: Int,
        val headers: List<List<String>> = emptyList(),
        val body: String,
    ) : HttpResult {
        override fun toJson(): JsonObject = buildJsonObject {
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
        }

        override fun encode(): String = encodeJson(toJson())
    }

    data class Error(val message: String) : HttpResult {
        override fun toJson(): JsonObject = buildJsonObject {
            put("status", JsonPrimitive("error"))
            put("message", JsonPrimitive(message))
        }

        override fun encode(): String = encodeJson(toJson())
    }

    companion object {
        private fun encodeJson(obj: JsonObject): String =
            CapabilityWire.json.encodeToString(JsonObject.serializer(), obj)
    }
}
