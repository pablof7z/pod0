package io.f7z.podcast.capabilities

/**
 * Single Android implementation of NMP's capability callback socket.
 *
 * Rust owns policy and issues `CapabilityRequest` envelopes. Android routes
 * by namespace, executes the requested OS work, and returns a
 * `CapabilityEnvelope`. No app state is cached here.
 */
class AndroidCapabilityRouter(
    private val audio: ExoPlayerCapability,
    private val http: HttpCapability,
) {
    fun handle(requestJson: String): String {
        val request = runCatching {
            CapabilityWire.json.decodeFromString(CapabilityRequest.serializer(), requestJson)
        }.getOrNull() ?: return CapabilityWire.error("", "", "malformed-request")

        return when (request.namespace) {
            ExoPlayerCapability.NAMESPACE -> {
                audio.handleCommand(request.payloadJson)
                CapabilityWire.ok(request.namespace, request.correlationId)
            }
            HttpCapability.NAMESPACE -> http.handle(request)
            HttpCapability.ASYNC_NAMESPACE -> http.handleAsync(request)
            DownloadCapability.NAMESPACE -> {
                // Android downloads are executed from the Rust-projected
                // `downloads.active` snapshot. Acknowledge the push command so
                // Rust does not see a capability failure, but keep one starter.
                CapabilityWire.ok(request.namespace, request.correlationId)
            }
            else -> CapabilityWire.error(
                namespace = request.namespace,
                correlationId = request.correlationId,
                message = "unknown-namespace",
            )
        }
    }
}
