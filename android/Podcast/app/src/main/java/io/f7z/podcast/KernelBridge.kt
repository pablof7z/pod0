package io.f7z.podcast

import io.f7z.podcast.capabilities.AndroidCapabilityRouter

/**
 * Thin JNI wrapper around `libnmp_app_podcast.so`, which links the SAME Rust
 * `nmp-app-podcast` crate the iOS app consumes as a `staticlib`. The native
 * symbols live in `apps/nmp-app-podcast/src/android.rs`
 * (gated `#[cfg(target_os = "android")]`) and mirror the iOS
 * `KernelBridge.swift` surface 1:1.
 *
 * **Doctrine — same as iOS:**
 *
 *  * D5/D8 — no business logic or cached state here; pure transport.
 *  * D6    — errors never cross FFI. Natives return only a handle, a string,
 *            or void. Outcomes arrive on the next JSON snapshot.
 *
 * **Why the JNI shim lives in the same crate (advisor decision):**
 *
 * The milestone calls for "same `cargo` binary used by iOS — no logic
 * forked". Keeping the JNI surface in `nmp-app-podcast` (vs. a separate
 * `nmp-android-ffi`-style crate like NMP/Chirp) means cargo-ndk packs both
 * the C ABI iOS consumes and the JNI ABI Android consumes into a single
 * `libnmp_app_podcast.so`. The downside — JNI symbol namespace
 * (`io.f7z.podcast.KernelBridge`) is hard-coded — is fine for a proof-of-
 * concept and will be factored out by M14 codegen.
 */
class KernelBridge {
    /** Opaque pointer to the Rust `Session` struct. 0 means freed/uninitialized. */
    private var handle: Long = 0

    init {
        System.loadLibrary(LIB_NAME)
        handle = nativeNew()
    }

    /** Start the kernel actor with the given snapshot cadence. */
    fun start(visibleLimit: Int = 80, emitHz: Int = 4) {
        if (handle != 0L) nativeStart(handle, visibleLimit, emitHz)
    }

    /** Halt the kernel actor (idempotent). */
    fun stop() {
        if (handle != 0L) nativeStop(handle)
    }

    /** Actor-liveness probe (D7). True while the Rust actor thread is alive. */
    fun isAlive(): Boolean = if (handle != 0L) nativeIsAlive(handle) == 1 else false

    /** G3 — host lifecycle → kernel lifecycle bridge. */
    fun lifecycleForeground() {
        if (handle != 0L) nativeLifecycleForeground(handle)
    }

    fun lifecycleBackground() {
        if (handle != 0L) nativeLifecycleBackground(handle)
    }

    /**
     * Generic namespace-keyed action dispatch. Mirrors the iOS
     * `dispatchAction(namespace:body:)` shape. Returns the kernel's JSON
     * envelope (`{"correlation_id":...}` on accept, `{"error":...}` on
     * rejection) or `null` on any FFI failure (D6).
     */
    fun dispatchAction(namespace: String, payloadJson: String): String? =
        if (handle != 0L) nativeDispatchAction(handle, namespace, payloadJson) else null

    /**
     * Namespace-agnostic action dispatch (M13.A stub). The action envelope
     * is `{"id":"podcast.player.play","payload":{...}}`; the Rust side
     * parses the id and (in M13.B) routes through the kernel action router.
     *
     * Returns `0` on success, `-1` on any parse/FFI failure (D6 — never
     * throws). The Kotlin call site treats both as "the kernel will tell
     * us what happened on the next snapshot tick".
     *
     * Unlike `dispatchAction`, this entry point is handle-agnostic — the
     * kernel state lives in Rust statics keyed by the namespace, which is
     * how the M13.B router will look up the destination actor.
     */
    external fun nmpActionDispatch(actionJson: String): Int

    /**
     * Register the Android capability router. Rust issues
     * `CapabilityRequest` envelopes through NMP's callback socket; the router
     * executes OS work (HTTP, ExoPlayer) and returns `CapabilityEnvelope` JSON.
     */
    fun registerCapabilityRouter(router: AndroidCapabilityRouter) {
        if (handle != 0L) nativeSetCapabilityRouter(handle, router)
    }

    fun unregisterCapabilityRouter() {
        if (handle != 0L) nativeSetCapabilityRouter(handle, null)
    }

    /**
     * Host → kernel report channel for capability observations. Audio reports
     * route through `nmp_app_podcast_audio_report`; the returned follow-up
     * command JSON, if any, should be executed by the audio capability.
     */
    fun capabilityReport(namespace: String, reportJson: String): String? =
        if (handle != 0L) nativeCapabilityReport(handle, namespace, reportJson) else null

    /**
     * Host → kernel **download**-report channel. The JSON-encoded `DownloadReport`
     * (`progress` / `completed` / `failed` / `cancelled` / `paused`) is
     * projected onto the kernel `DownloadQueue`, and any follow-up
     * `DownloadCommand` the queue emits (e.g. `start_download` for the next
     * waiting item once a slot frees) is returned as a JSON `String`, or
     * `null` when there is none / on any FFI failure (D6).
     *
     * This is the Android analogue of the iOS
     * `KernelBridge+Callbacks.swift::attachDownloadReportChannel`
     * return-and-execute pattern. Android deliberately starts downloads from
     * projected `downloads.active` rows so there is one starter/canceller and
     * no duplicate path competing with the Rust queue.
     */
    fun downloadReport(reportJson: String): String? =
        if (handle != 0L) nativeDownloadReport(handle, reportJson) else null

    /**
     * One-shot sign-in via local nsec. Demonstrates a single capability hop
     * the milestone exit checklist calls for. The PoC UI passes a stub value;
     * a real implementation would prompt for the nsec and route through the
     * Keychain capability.
     */
    fun signinNsec(nsec: String) {
        if (handle != 0L) nativeSigninNsec(handle, nsec)
    }

    /**
     * Blocking (≤250 ms) drain of the kernel snapshot channel; `null` on idle.
     * Mirrors the Swift push callback's cadence via a pull-side model — see
     * `apps/nmp-app-podcast/src/android.rs` for the rationale.
     */
    fun nextUpdate(): String? = if (handle != 0L) nativeNextUpdate(handle) else null

    /** Pull the Podcast projection JSON (one-shot, off the projection cache). */
    fun podcastSnapshot(): String? = if (handle != 0L) nativePodcastSnapshot(handle) else null

    /**
     * Shared agent chat completion transport. Android sends the same message
     * array contract as iOS; Rust owns provider/model routing, credentials,
     * tool-loop handling, and error reporting.
     */
    fun chatComplete(messagesJson: String): String? =
        if (handle != 0L) nativeChatComplete(handle, messagesJson) else null

    /**
     * Shared provider completion transport. The JSON intent and JSON envelope
     * are the same provider-neutral contract iOS passes through
     * `nmp_app_podcast_provider_complete`; Android owns no provider HTTP here.
     */
    fun providerComplete(intentJson: String): String? =
        if (handle != 0L) nativeProviderComplete(handle, intentJson) else null

    /** Shared provider embedding transport; returns Rust's JSON envelope. */
    fun providerEmbed(intentJson: String): String? =
        if (handle != 0L) nativeProviderEmbed(handle, intentJson) else null

    /**
     * Shared online-search transport. Rust owns Perplexity/OpenRouter request
     * shaping, credentials, status mapping, and response parsing.
     */
    fun perplexitySearch(intentJson: String): String? =
        if (handle != 0L) nativePerplexitySearch(handle, intentJson) else null

    /**
     * Shared provider model catalog. Rust owns OpenRouter/models.dev/Ollama
     * retrieval and normalization; Android receives the JSON envelope only.
     */
    fun providerModelCatalog(): String? =
        if (handle != 0L) nativeProviderModelCatalog(handle) else null

    /**
     * Shared speech STT/TTS model catalog. Rust owns the option sets; Android
     * receives the JSON envelope only.
     */
    fun speechModelCatalog(): String? =
        if (handle != 0L) nativeSpeechModelCatalog(handle) else null

    /**
     * Shared OpenRouter key validation. Rust owns `/auth/key`, credentials,
     * request shaping, and response parsing; Android receives the JSON envelope.
     */
    fun validateOpenRouterKey(): String? =
        if (handle != 0L) nativeValidateOpenRouterKey(handle) else null

    /**
     * Shared ElevenLabs key validation. Rust owns `/v1/user`, credentials,
     * request shaping, and response parsing; Android receives the JSON envelope.
     */
    fun validateElevenLabsKey(): String? =
        if (handle != 0L) nativeValidateElevenLabsKey(handle) else null

    /**
     * Shared ElevenLabs voice catalog. Rust owns `/v1/voices`, credentials,
     * request shaping, status mapping, and response parsing.
     */
    fun elevenLabsVoiceCatalog(): String? =
        if (handle != 0L) nativeElevenLabsVoiceCatalog(handle) else null

    /**
     * Shared ElevenLabs one-shot text-to-speech transport. Android supplies
     * text/voice/model intent only; Rust owns credentials, request shaping,
     * provider errors, and audio response normalization.
     */
    fun elevenLabsTextToSpeech(intentJson: String): String? =
        if (handle != 0L) nativeElevenLabsTextToSpeech(handle, intentJson) else null

    /**
     * Shared OpenRouter Whisper transcription transport. Android supplies only
     * the typed audio-source intent; Rust owns OpenRouter HTTP and credentials.
     */
    fun openRouterWhisperTranscribe(intentJson: String): String? =
        if (handle != 0L) nativeOpenRouterWhisperTranscribe(handle, intentJson) else null

    /**
     * Shared ElevenLabs Scribe transcription transport. Android supplies only
     * the typed audio-source intent; Rust owns ElevenLabs HTTP and credentials.
     */
    fun elevenLabsScribeTranscribe(intentJson: String): String? =
        if (handle != 0L) nativeElevenLabsScribeTranscribe(handle, intentJson) else null

    /**
     * Shared AssemblyAI transcription transport. Android supplies only the
     * typed audio-source intent; Rust owns AssemblyAI submit/poll HTTP and
     * credentials.
     */
    fun assemblyAITranscribe(intentJson: String): String? =
        if (handle != 0L) nativeAssemblyAITranscribe(handle, intentJson) else null

    /** Shared provider image generation transport; returns Rust's JSON envelope. */
    fun generateImage(requestJson: String): String? =
        if (handle != 0L) nativeGenerateImage(handle, requestJson) else null

    /** Shared RAG reranking transport; returns Rust's JSON envelope. */
    fun rerank(requestJson: String): String? =
        if (handle != 0L) nativeRerank(handle, requestJson) else null

    /** Tear down the kernel and projection handle. Exactly-once. */
    fun free() {
        if (handle != 0L) {
            nativeFree(handle)
            handle = 0
        }
    }

    // ── External natives — must exactly match the JNI exports in
    //    `apps/nmp-app-podcast/src/android.rs`. The JNI loader resolves these
    //    against symbols of the form `Java_io_f7z_podcast_KernelBridge_<name>`.
    private external fun nativeNew(): Long
    private external fun nativeStart(handle: Long, visibleLimit: Int, emitHz: Int)
    private external fun nativeStop(handle: Long)
    private external fun nativeIsAlive(handle: Long): Int
    private external fun nativeLifecycleForeground(handle: Long)
    private external fun nativeLifecycleBackground(handle: Long)
    private external fun nativeDispatchAction(handle: Long, namespace: String, payload: String): String?
    private external fun nativeSetCapabilityRouter(handle: Long, router: AndroidCapabilityRouter?)
    private external fun nativeCapabilityReport(handle: Long, namespace: String, reportJson: String): String?
    private external fun nativeDownloadReport(handle: Long, reportJson: String): String?
    private external fun nativeSigninNsec(handle: Long, nsec: String)
    private external fun nativeNextUpdate(handle: Long): String?
    private external fun nativePodcastSnapshot(handle: Long): String?
    private external fun nativeChatComplete(handle: Long, messagesJson: String): String?
    private external fun nativeProviderComplete(handle: Long, intentJson: String): String?
    private external fun nativeProviderEmbed(handle: Long, intentJson: String): String?
    private external fun nativePerplexitySearch(handle: Long, intentJson: String): String?
    private external fun nativeProviderModelCatalog(handle: Long): String?
    private external fun nativeSpeechModelCatalog(handle: Long): String?
    private external fun nativeValidateOpenRouterKey(handle: Long): String?
    private external fun nativeValidateElevenLabsKey(handle: Long): String?
    private external fun nativeElevenLabsVoiceCatalog(handle: Long): String?
    private external fun nativeElevenLabsTextToSpeech(handle: Long, intentJson: String): String?
    private external fun nativeOpenRouterWhisperTranscribe(handle: Long, intentJson: String): String?
    private external fun nativeElevenLabsScribeTranscribe(handle: Long, intentJson: String): String?
    private external fun nativeAssemblyAITranscribe(handle: Long, intentJson: String): String?
    private external fun nativeGenerateImage(handle: Long, requestJson: String): String?
    private external fun nativeRerank(handle: Long, requestJson: String): String?
    private external fun nativeFree(handle: Long)

    companion object {
        /**
         * Matches the Rust `[lib] name = "nmp_app_podcast"`. `System.loadLibrary`
         * strips the `lib` prefix and `.so` suffix, so we pass the bare name.
         */
        private const val LIB_NAME = "nmp_app_podcast"
    }
}
