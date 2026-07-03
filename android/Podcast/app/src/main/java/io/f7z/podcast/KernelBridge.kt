package io.f7z.podcast

import io.f7z.podcast.capabilities.AndroidCapabilityRouter
import io.f7z.podcast.capabilities.CapabilityRequest
import io.f7z.podcast.capabilities.CapabilityWire
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.atomic.AtomicBoolean
import uniffi.nmp_app_podcast.PodcastApp as UniFfiPodcastApp
import uniffi.nmp_app_podcast.PodcastCapabilitySink
import uniffi.nmp_app_podcast.PodcastProfileShape
import uniffi.nmp_app_podcast.PodcastRefLiveness
import uniffi.nmp_app_podcast.PodcastRefNamespace
import uniffi.nmp_app_podcast.PodcastRefShape
import uniffi.nmp_app_podcast.PodcastUpdateSink
import uniffi.nmp_app_podcast.podcastBridgeGlobalCall

/**
 * Android bridge around the app-owned generated UniFFI [UniFfiPodcastApp].
 *
 * Generated UniFFI owns generic runtime lifecycle, update/capability callbacks,
 * identity, NIP-46, NIP-55, ref resolution, and app-domain bridge calls.
 */
class KernelBridge : KernelDispatcher {
    private val app = UniFfiPodcastApp()
    private val closed = AtomicBoolean(false)
    private val updateQueue = LinkedBlockingQueue<UpdateQueueItem>(MAX_QUEUED_UPDATES)
    private val signerQueue = LinkedBlockingQueue<SignerQueueItem>(MAX_QUEUED_SIGNER_REQUESTS)

    @Volatile
    private var capabilityRouter: AndroidCapabilityRouter? = null

    private val updateSink = object : PodcastUpdateSink {
        override fun onUpdate(frame: ByteArray) {
            if (closed.get()) return
            app.decodeUpdateFrame(frame)?.let { decoded ->
                putUpdate(UpdateQueueItem.Frame(decoded))
            }
        }
    }

    private val capabilitySink = object : PodcastCapabilitySink {
        override fun onCapabilityRequest(requestJson: String): String =
            routeCapabilityRequest(requestJson)
    }

    init {
        app.signerBrokerInit()
        app.setUpdateSink(updateSink)
    }

    fun setDataDir(path: String) {
        if (isOpen()) app.setPodcastDataDir(path)
    }

    fun start(visibleLimit: Int = 80, emitHz: Int = 4) {
        if (!isOpen()) return
        app.consumeAllBuiltinProjections()
        app.start(visibleLimit.toUInt(), emitHz.toUInt())
    }

    fun stop() {
        if (isOpen()) app.stop()
    }

    fun isAlive(): Boolean = isOpen() && app.isAlive()

    fun lifecycleForeground() {
        if (isOpen()) app.lifecycleForeground()
    }

    fun lifecycleBackground() {
        if (isOpen()) app.lifecycleBackground()
    }

    override fun dispatchAction(namespace: String, payloadJson: String): String? {
        return if (isOpen()) app.dispatchPodcastAction(namespace, payloadJson) else null
    }

    fun registerCapabilityRouter(router: AndroidCapabilityRouter) {
        if (!isOpen()) return
        capabilityRouter = router
        app.setCapabilityCallback(capabilitySink)
    }

    fun unregisterCapabilityRouter() {
        capabilityRouter = null
        if (isOpen()) app.setCapabilityCallback(null)
    }

    fun capabilityReport(namespace: String, reportJson: String): String? {
        val endpoint = when (namespace) {
            "audio" -> "nmp_app_podcast_audio_report"
            "download" -> "nmp_app_podcast_download_report"
            else -> return null
        }
        return if (isOpen()) app.podcastBridgeCall(endpoint, reportJson) else null
    }

    fun downloadReport(reportJson: String): String? {
        return if (isOpen()) app.podcastBridgeCall("nmp_app_podcast_download_report", reportJson) else null
    }

    fun httpReport(reportJson: String) {
        if (isOpen()) app.podcastBridgeCall("nmp_app_podcast_http_report", reportJson)
    }

    fun signinNsec(nsec: String) {
        if (isOpen()) app.signinNsec(nsec, makeActive = true)
    }

    fun nextUpdate(): String? =
        when (val item = updateQueue.take()) {
            is UpdateQueueItem.Frame -> item.json
            UpdateQueueItem.Shutdown -> null
        }

    fun signInBunker(uri: String, makeActive: Boolean = true) {
        if (isOpen()) app.signinBunker(uri, makeActive)
    }

    fun cancelBunkerHandshake() {
        if (isOpen()) app.cancelBunkerHandshake()
    }

    @Suppress("UNUSED_PARAMETER")
    fun nostrconnectUri(relayUrl: String? = null, callbackScheme: String? = null): String? {
        return if (isOpen()) app.nostrconnectUri(callbackScheme) else null
    }

    fun signInNip55(signerPackage: String?) {
        if (isOpen()) app.signinNip55(signerPackage)
    }

    fun nextSignerRequest(): String? =
        when (val item = signerQueue.take()) {
            is SignerQueueItem.Request -> item.json
            SignerQueueItem.Shutdown -> null
        }

    fun deliverSignerResponse(responseJson: String) {
        if (isOpen()) app.deliverExternalSignerResponse(responseJson)
    }

    fun claimProfile(pubkeyHex: String, consumerID: String) {
        if (!isOpen()) return
        app.resolveRef(
            namespace = PodcastRefNamespace.PROFILE,
            key = pubkeyHex,
            consumerId = consumerID,
            shape = PodcastRefShape.Profile(PodcastProfileShape.CARD),
            liveness = PodcastRefLiveness.CACHE_OK,
        )
    }

    fun releaseProfile(pubkeyHex: String, consumerID: String) {
        if (!isOpen()) return
        app.releaseRef(
            namespace = PodcastRefNamespace.PROFILE,
            key = pubkeyHex,
            consumerId = consumerID,
        )
    }

    fun podcastSnapshot(): String? {
        return if (isOpen()) app.podcastSnapshot() else null
    }

    fun chatComplete(messagesJson: String): String? {
        return if (isOpen()) app.podcastBridgeCall("nmp_app_podcast_chat_complete", messagesJson) else null
    }

    fun providerComplete(intentJson: String): String? {
        return if (isOpen()) app.podcastBridgeCall("nmp_app_podcast_provider_complete", intentJson) else null
    }

    fun providerEmbed(intentJson: String): String? {
        return if (isOpen()) app.podcastBridgeCall("nmp_app_podcast_provider_embed", intentJson) else null
    }

    fun perplexitySearch(intentJson: String): String? {
        return if (isOpen()) app.podcastBridgeCall("nmp_app_podcast_perplexity_search", intentJson) else null
    }

    fun providerModelCatalog(): String? {
        return if (isOpen()) app.podcastBridgeCall("nmp_app_podcast_provider_model_catalog", null) else null
    }

    fun speechModelCatalog(): String? {
        return if (isOpen()) app.podcastBridgeCall("nmp_app_podcast_speech_model_catalog", null) else null
    }

    fun localModelCatalog(): String? {
        return if (isOpen()) app.podcastBridgeCall("nmp_app_podcast_local_model_catalog", null) else null
    }

    fun byokAuthorization(intentJson: String): String? =
        podcastBridgeGlobalCall("nmp_app_podcast_byok_authorization", intentJson)

    fun byokExchange(intentJson: String): String? {
        return if (isOpen()) app.podcastBridgeCall("nmp_app_podcast_byok_exchange", intentJson) else null
    }

    fun validateOpenRouterKey(): String? {
        return if (isOpen()) app.podcastBridgeCall("nmp_app_podcast_validate_openrouter_key", null) else null
    }

    fun validateElevenLabsKey(): String? {
        return if (isOpen()) app.podcastBridgeCall("nmp_app_podcast_validate_elevenlabs_key", null) else null
    }

    fun elevenLabsVoiceCatalog(): String? {
        return if (isOpen()) app.podcastBridgeCall("nmp_app_podcast_elevenlabs_voice_catalog", null) else null
    }

    fun elevenLabsTextToSpeech(intentJson: String): String? {
        return if (isOpen()) app.podcastBridgeCall("nmp_app_podcast_elevenlabs_tts_synthesize", intentJson) else null
    }

    fun openRouterWhisperTranscribe(intentJson: String): String? {
        return if (isOpen()) app.podcastBridgeCall("nmp_app_podcast_openrouter_whisper_transcribe", intentJson) else null
    }

    fun elevenLabsScribeTranscribe(intentJson: String): String? {
        return if (isOpen()) app.podcastBridgeCall("nmp_app_podcast_elevenlabs_scribe_transcribe", intentJson) else null
    }

    fun assemblyAITranscribe(intentJson: String): String? {
        return if (isOpen()) app.podcastBridgeCall("nmp_app_podcast_assemblyai_transcribe", intentJson) else null
    }

    fun generateImage(requestJson: String): String? {
        return if (isOpen()) app.podcastBridgeCall("nmp_app_podcast_generate_image", requestJson) else null
    }

    fun rerank(requestJson: String): String? {
        return if (isOpen()) app.podcastBridgeCall("nmp_app_podcast_rerank", requestJson) else null
    }

    fun free() {
        if (!closed.compareAndSet(false, true)) return
        capabilityRouter = null
        runCatching { app.setCapabilityCallback(null) }
        runCatching { app.setUpdateSink(null) }
        runCatching { app.stop() }
        runCatching { app.shutdown() }
        updateQueue.clear()
        signerQueue.clear()
        updateQueue.offer(UpdateQueueItem.Shutdown)
        signerQueue.offer(SignerQueueItem.Shutdown)
    }

    private fun routeCapabilityRequest(requestJson: String): String {
        if (closed.get()) return CapabilityWire.error("", "", "session-closed")
        val request = runCatching {
            CapabilityWire.json.decodeFromString(CapabilityRequest.serializer(), requestJson)
        }.getOrNull()
        if (request?.namespace == EXTERNAL_SIGNER_NAMESPACE) {
            putSignerRequest(SignerQueueItem.Request(request.payloadJson))
            return CapabilityWire.envelope(
                namespace = EXTERNAL_SIGNER_NAMESPACE,
                correlationId = request.correlationId,
                resultJson = """{"status":"dispatched"}""",
            )
        }
        return capabilityRouter?.handle(requestJson)
            ?: CapabilityWire.error(
                namespace = request?.namespace.orEmpty(),
                correlationId = request?.correlationId.orEmpty(),
                message = if (request == null) "malformed-request" else "router-not-registered",
            )
    }

    private fun isOpen(): Boolean = !closed.get()

    private fun putUpdate(item: UpdateQueueItem) {
        runCatching { updateQueue.put(item) }
            .onFailure { Thread.currentThread().interrupt() }
    }

    private fun putSignerRequest(item: SignerQueueItem) {
        runCatching { signerQueue.put(item) }
            .onFailure { Thread.currentThread().interrupt() }
    }

    private sealed interface UpdateQueueItem {
        data class Frame(val json: String) : UpdateQueueItem
        object Shutdown : UpdateQueueItem
    }

    private sealed interface SignerQueueItem {
        data class Request(val json: String) : SignerQueueItem
        object Shutdown : SignerQueueItem
    }

    companion object {
        private const val LIB_NAME = "nmp_app_podcast"
        private const val EXTERNAL_SIGNER_NAMESPACE = "external_signer"
        private const val MAX_QUEUED_UPDATES = 256
        private const val MAX_QUEUED_SIGNER_REQUESTS = 32

        init {
            System.loadLibrary(LIB_NAME)
        }
    }
}
