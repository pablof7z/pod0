package io.f7z.podcast

import io.f7z.podcast.capabilities.AndroidCapabilityRouter
import java.util.concurrent.atomic.AtomicLong

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
    private val handle = AtomicLong(0L)

    init {
        System.loadLibrary(LIB_NAME)
        handle.set(nativeNew())
    }

    /**
     * Bind the kernel's podcast library store to a persistence directory and
     * reload any saved state (`podcasts.json`, `identity.json`, the Up-Next
     * queue, per-podcast keys, relay config, inbox-triage cache).
     *
     * Mirror of the iOS `KernelBridge.swift` `set_data_dir` call: invoke once,
     * after construction (`nativeNew` already ran `register`) and **before**
     * [start], so the actor reloads persisted state before it emits its first
     * snapshot. Pass `context.filesDir.absolutePath`. The kernel owns what and
     * when to persist; this shell only supplies the OS path.
     */
    fun setDataDir(path: String) {
        val h = handle.get(); if (h != 0L) nativeSetDataDir(h, path)
    }

    /** Start the kernel actor with the given snapshot cadence. */
    fun start(visibleLimit: Int = 80, emitHz: Int = 4) {
        val h = handle.get(); if (h != 0L) nativeStart(h, visibleLimit, emitHz)
    }

    /** Halt the kernel actor (idempotent). */
    fun stop() {
        val h = handle.get(); if (h != 0L) nativeStop(h)
    }

    /** Actor-liveness probe (D7). True while the Rust actor thread is alive. */
    fun isAlive(): Boolean { val h = handle.get(); return if (h != 0L) nativeIsAlive(h) == 1 else false }

    /** G3 — host lifecycle → kernel lifecycle bridge. */
    fun lifecycleForeground() {
        val h = handle.get(); if (h != 0L) nativeLifecycleForeground(h)
    }

    fun lifecycleBackground() {
        val h = handle.get(); if (h != 0L) nativeLifecycleBackground(h)
    }

    /**
     * Generic namespace-keyed action dispatch. Mirrors the iOS
     * `dispatchAction(namespace:body:)` shape. Returns the kernel's JSON
     * envelope (`{"correlation_id":...}` on accept, `{"error":...}` on
     * rejection) or `null` on any FFI failure (D6).
     */
    fun dispatchAction(namespace: String, payloadJson: String): String? {
        val h = handle.get(); return if (h != 0L) nativeDispatchAction(h, namespace, payloadJson) else null
    }

    /**
     * Register the Android capability router. Rust issues
     * `CapabilityRequest` envelopes through NMP's callback socket; the router
     * executes OS work (HTTP, ExoPlayer) and returns `CapabilityEnvelope` JSON.
     */
    fun registerCapabilityRouter(router: AndroidCapabilityRouter) {
        val h = handle.get(); if (h != 0L) nativeSetCapabilityRouter(h, router)
    }

    fun unregisterCapabilityRouter() {
        val h = handle.get(); if (h != 0L) nativeSetCapabilityRouter(h, null)
    }

    /**
     * Host → kernel report channel for capability observations. Audio reports
     * route through `nmp_app_podcast_audio_report`; the returned follow-up
     * command JSON, if any, should be executed by the audio capability.
     */
    fun capabilityReport(namespace: String, reportJson: String): String? {
        val h = handle.get(); return if (h != 0L) nativeCapabilityReport(h, namespace, reportJson) else null
    }

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
    fun downloadReport(reportJson: String): String? {
        val h = handle.get(); return if (h != 0L) nativeDownloadReport(h, reportJson) else null
    }

    /**
     * Host -> kernel **async HTTP**-report channel for the optimistic-subscribe
     * feed fetch. The Android `HttpCapability` async executor runs the RSS
     * request off the actor thread and posts the JSON-encoded `HttpReport`
     * here. Unlike downloads there is no follow-up command (the kernel hydrates
     * the row and bumps the snapshot rev), so this returns nothing. The
     * `handle != 0L` guard is the report-after-`free()` protection, mirroring
     * `downloadReport`.
     */
    fun httpReport(reportJson: String) {
        val h = handle.get(); if (h != 0L) nativeHttpReport(h, reportJson)
    }

    /**
     * One-shot sign-in via local nsec. Demonstrates a single capability hop
     * the milestone exit checklist calls for. The PoC UI passes a stub value;
     * a real implementation would prompt for the nsec and route through the
     * Keychain capability.
     */
    fun signinNsec(nsec: String) {
        val h = handle.get(); if (h != 0L) nativeSigninNsec(h, nsec)
    }

    /**
     * Blocking drain of the kernel snapshot channel. Blocks until a frame
     * arrives or the session is shut down. Mirrors the Swift push callback's
     * cadence via a pull-side model — see `apps/nmp-app-podcast/src/android.rs`
     * for the rationale. Returns `null` only on session shutdown.
     */
    fun nextUpdate(): String? { val h = handle.get(); return if (h != 0L) nativeNextUpdate(h) else null }

    // ── NIP-46 remote signer (bunker:// + nostrconnect://) ─────────────────

    /**
     * Enqueue `ActorCommand::SignInBunker` with the supplied `bunker://` URI.
     * Silent no-op (D6) if the URI is malformed; the kernel validates it.
     * The handshake result surfaces on the next snapshot tick as an EXTERNAL
     * (remote-signer) `activeAccount` — the projection emits it as mode
     * `"nip55"`, never a distinct "bunker" token (D6 — fire-and-forget, no
     * return value). Detect completion via `Nip46Uri.handshakeCompleted`.
     *
     * Mirrors iOS `PodcastHandle.signInBunker(uri:)`.
     */
    fun signInBunker(uri: String, makeActive: Boolean = true) {
        val h = handle.get(); if (h != 0L) nativeSignInBunker(h, uri, if (makeActive) 1 else 0)
    }

    /**
     * Cancel the in-flight NIP-46 handshake. Idempotent / safe when no
     * handshake is in flight (D6 — silent no-op in that case).
     *
     * Mirrors iOS `PodcastHandle.cancelBunkerHandshake()`.
     */
    fun cancelBunkerHandshake() {
        val h = handle.get(); if (h != 0L) nativeCancelBunkerHandshake(h)
    }

    /**
     * Generate a brand-new `nostrconnect://` URI from the broker. Returns the
     * URI string, or `null` when the broker is not initialised or Rust returns
     * a null pointer (D6).
     *
     * `relayUrl` — pass `null` to let the kernel pick the first write-capable
     * relay from its relay-edit projection.
     * `callbackScheme` — pass `null` unless the host's URL scheme is registered
     * with the OS (Android deep link); when non-null Rust appends a
     * percent-encoded `&callback=<scheme>` query parameter.
     *
     * Mirrors iOS `PodcastHandle.nostrconnectURI(relayURL:callbackScheme:)`.
     */
    fun nostrconnectUri(relayUrl: String? = null, callbackScheme: String? = null): String? {
        val h = handle.get(); return if (h != 0L) nativeNostrconnectUri(h, relayUrl, callbackScheme) else null
    }

    // ── NIP-55 external signer (ADR-0048) ──────────────────────────────────

    /**
     * Begin a NIP-55 (Amber) sign-in. Rust builds the `get_public_key` +
     * permission-batch request and emits it onto the capability socket; the
     * trampoline routes it to the signer-request channel, where a reader thread
     * (see [nextSignerRequest]) drains it and fires the Amber Intent. The
     * resulting pubkey-only account appears on the next snapshot tick (D6 — the
     * outcome arrives as state, never a return value).
     *
     * @param signerPackage the Android package of the chosen signer
     *   (`com.greenart7c3.nostrsigner` for Amber), or `null` to let the OS
     *   resolver pick.
     */
    fun signInNip55(signerPackage: String?) {
        val h = handle.get(); if (h != 0L) nativeSignInNip55(h, signerPackage)
    }

    /**
     * Blocking drain of the outbound NIP-55 request channel. Blocks until a
     * request arrives or the session is shut down. The signer analogue of
     * [nextUpdate]: a dedicated reader thread loops on this and hands each
     * `ExternalSignerRequest` JSON to `ExternalSignerCapabilityBridge.handleJson`.
     * Returns `null` only on session shutdown.
     */
    fun nextSignerRequest(): String? { val h = handle.get(); return if (h != 0L) nativeNextSignerRequest(h) else null }

    /**
     * Report a raw `ExternalSignerResponse` JSON (Amber's reply) back to the
     * Rust driver, which owns correlation routing and all policy (D7 —
     * verbatim, no interpretation here).
     */
    fun deliverSignerResponse(responseJson: String) {
        val h = handle.get(); if (h != 0L) nativeDeliverSignerResponse(h, responseJson)
    }

    /**
     * Register a refcounted interest in `pubkeyHex`'s kind:0 profile under
     * `consumerID`. The kernel fetches the profile over its own relay pool (cold
     * claim) and surfaces it in `projections["resolved_profiles"]` on the next
     * push frame. Fire-and-forget (D6): an invalid pubkey is a silent no-op.
     *
     * `pubkeyHex` MUST be lowercase hex. `consumerID` is a stable per-view token
     * so claims dedupe and release matches claim.
     *
     * Mirrors iOS `PodcastHandle.claimProfile(pubkeyHex:consumerID:)` and the
     * `nmp_app_claim_profile` C-ABI symbol declared in `NmpCore.h`.
     */
    fun claimProfile(pubkeyHex: String, consumerID: String) {
        val h = handle.get(); if (h != 0L) nativeClaimProfile(h, pubkeyHex, consumerID)
    }

    /**
     * Release a previously-claimed profile interest. The kernel drops the
     * pending request when the last consumer releases. Idempotent / safe when
     * nothing is claimed for this pubkey+consumer pair (D6 — silent no-op).
     *
     * Mirrors iOS `PodcastHandle.releaseProfile(pubkeyHex:consumerID:)` and the
     * `nmp_app_release_profile` C-ABI symbol declared in `NmpCore.h`.
     */
    fun releaseProfile(pubkeyHex: String, consumerID: String) {
        val h = handle.get(); if (h != 0L) nativeReleaseProfile(h, pubkeyHex, consumerID)
    }

    /** Pull the Podcast projection JSON (one-shot, off the projection cache). */
    fun podcastSnapshot(): String? { val h = handle.get(); return if (h != 0L) nativePodcastSnapshot(h) else null }

    /**
     * Shared agent chat completion transport. Android sends the same message
     * array contract as iOS; Rust owns provider/model routing, credentials,
     * tool-loop handling, and error reporting.
     */
    fun chatComplete(messagesJson: String): String? {
        val h = handle.get(); return if (h != 0L) nativeChatComplete(h, messagesJson) else null
    }

    /**
     * Shared provider completion transport. The JSON intent and JSON envelope
     * are the same provider-neutral contract iOS passes through
     * `nmp_app_podcast_provider_complete`; Android owns no provider HTTP here.
     */
    fun providerComplete(intentJson: String): String? {
        val h = handle.get(); return if (h != 0L) nativeProviderComplete(h, intentJson) else null
    }

    /** Shared provider embedding transport; returns Rust's JSON envelope. */
    fun providerEmbed(intentJson: String): String? {
        val h = handle.get(); return if (h != 0L) nativeProviderEmbed(h, intentJson) else null
    }

    /**
     * Shared online-search transport. Rust owns Perplexity/OpenRouter request
     * shaping, credentials, status mapping, and response parsing.
     */
    fun perplexitySearch(intentJson: String): String? {
        val h = handle.get(); return if (h != 0L) nativePerplexitySearch(h, intentJson) else null
    }

    /**
     * Shared provider model catalog. Rust owns OpenRouter/models.dev/Ollama
     * retrieval and normalization; Android receives the JSON envelope only.
     */
    fun providerModelCatalog(): String? {
        val h = handle.get(); return if (h != 0L) nativeProviderModelCatalog(h) else null
    }

    /**
     * Shared speech STT/TTS model catalog. Rust owns the option sets; Android
     * receives the JSON envelope only.
     */
    fun speechModelCatalog(): String? {
        val h = handle.get(); return if (h != 0L) nativeSpeechModelCatalog(h) else null
    }

    /**
     * Shared on-device model catalog. Rust owns model ids, display metadata,
     * download URLs, sizes, and RAM floors; Android receives the JSON envelope.
     */
    fun localModelCatalog(): String? {
        val h = handle.get(); return if (h != 0L) nativeLocalModelCatalog(h) else null
    }

    /**
     * Shared BYOK authorization helper. Android supplies app/browser facts;
     * Rust owns provider scopes, PKCE state/verifier generation, and URL
     * construction.
     */
    fun byokAuthorization(intentJson: String): String? =
        nativeByokAuthorization(intentJson)

    /**
     * Shared BYOK token exchange. Android supplies the Rust-created pending
     * auth and browser callback URL; Rust validates state/callback and owns
     * `/api/token` request/response parsing.
     */
    fun byokExchange(intentJson: String): String? {
        val h = handle.get(); return if (h != 0L) nativeByokExchange(h, intentJson) else null
    }

    /**
     * Shared OpenRouter key validation. Rust owns `/auth/key`, credentials,
     * request shaping, and response parsing; Android receives the JSON envelope.
     */
    fun validateOpenRouterKey(): String? {
        val h = handle.get(); return if (h != 0L) nativeValidateOpenRouterKey(h) else null
    }

    /**
     * Shared ElevenLabs key validation. Rust owns `/v1/user`, credentials,
     * request shaping, and response parsing; Android receives the JSON envelope.
     */
    fun validateElevenLabsKey(): String? {
        val h = handle.get(); return if (h != 0L) nativeValidateElevenLabsKey(h) else null
    }

    /**
     * Shared ElevenLabs voice catalog. Rust owns `/v1/voices`, credentials,
     * request shaping, status mapping, and response parsing.
     */
    fun elevenLabsVoiceCatalog(): String? {
        val h = handle.get(); return if (h != 0L) nativeElevenLabsVoiceCatalog(h) else null
    }

    /**
     * Shared ElevenLabs one-shot text-to-speech transport. Android supplies
     * text/voice/model intent only; Rust owns credentials, request shaping,
     * provider errors, and audio response normalization.
     */
    fun elevenLabsTextToSpeech(intentJson: String): String? {
        val h = handle.get(); return if (h != 0L) nativeElevenLabsTextToSpeech(h, intentJson) else null
    }

    /**
     * Shared OpenRouter Whisper transcription transport. Android supplies only
     * the typed audio-source intent; Rust owns OpenRouter HTTP and credentials.
     */
    fun openRouterWhisperTranscribe(intentJson: String): String? {
        val h = handle.get(); return if (h != 0L) nativeOpenRouterWhisperTranscribe(h, intentJson) else null
    }

    /**
     * Shared ElevenLabs Scribe transcription transport. Android supplies only
     * the typed audio-source intent; Rust owns ElevenLabs HTTP and credentials.
     */
    fun elevenLabsScribeTranscribe(intentJson: String): String? {
        val h = handle.get(); return if (h != 0L) nativeElevenLabsScribeTranscribe(h, intentJson) else null
    }

    /**
     * Shared AssemblyAI transcription transport. Android supplies only the
     * typed audio-source intent; Rust owns AssemblyAI submit/poll HTTP and
     * credentials.
     */
    fun assemblyAITranscribe(intentJson: String): String? {
        val h = handle.get(); return if (h != 0L) nativeAssemblyAITranscribe(h, intentJson) else null
    }

    /** Shared provider image generation transport; returns Rust's JSON envelope. */
    fun generateImage(requestJson: String): String? {
        val h = handle.get(); return if (h != 0L) nativeGenerateImage(h, requestJson) else null
    }

    /** Shared RAG reranking transport; returns Rust's JSON envelope. */
    fun rerank(requestJson: String): String? {
        val h = handle.get(); return if (h != 0L) nativeRerank(h, requestJson) else null
    }

    /** Tear down the kernel and projection handle. Exactly-once. */
    fun free() {
        // Zero the handle FIRST so session_ref callers racing with teardown
        // see 0 and bail out before we drop the Arc in nativeFree (#600).
        val h = handle.getAndSet(0L)
        if (h != 0L) nativeFree(h)
    }

    // ── External natives — must exactly match the JNI exports in
    //    `apps/nmp-app-podcast/src/android.rs`. The JNI loader resolves these
    //    against symbols of the form `Java_io_f7z_podcast_KernelBridge_<name>`.
    private external fun nativeNew(): Long
    private external fun nativeSetDataDir(handle: Long, path: String)
    private external fun nativeStart(handle: Long, visibleLimit: Int, emitHz: Int)
    private external fun nativeStop(handle: Long)
    private external fun nativeIsAlive(handle: Long): Int
    private external fun nativeLifecycleForeground(handle: Long)
    private external fun nativeLifecycleBackground(handle: Long)
    private external fun nativeDispatchAction(handle: Long, namespace: String, payload: String): String?
    private external fun nativeSetCapabilityRouter(handle: Long, router: AndroidCapabilityRouter?)
    private external fun nativeCapabilityReport(handle: Long, namespace: String, reportJson: String): String?
    private external fun nativeDownloadReport(handle: Long, reportJson: String): String?
    private external fun nativeHttpReport(handle: Long, reportJson: String)
    private external fun nativeSigninNsec(handle: Long, nsec: String)
    private external fun nativeSignInBunker(handle: Long, uri: String, makeActive: Int)
    private external fun nativeCancelBunkerHandshake(handle: Long)
    private external fun nativeNostrconnectUri(handle: Long, relayUrl: String?, callbackScheme: String?): String?
    private external fun nativeSignInNip55(handle: Long, signerPackage: String?)
    private external fun nativeNextSignerRequest(handle: Long): String?
    private external fun nativeDeliverSignerResponse(handle: Long, responseJson: String)
    private external fun nativeNextUpdate(handle: Long): String?
    private external fun nativeClaimProfile(handle: Long, pubkeyHex: String, consumerID: String)
    private external fun nativeReleaseProfile(handle: Long, pubkeyHex: String, consumerID: String)
    private external fun nativePodcastSnapshot(handle: Long): String?
    private external fun nativeChatComplete(handle: Long, messagesJson: String): String?
    private external fun nativeProviderComplete(handle: Long, intentJson: String): String?
    private external fun nativeProviderEmbed(handle: Long, intentJson: String): String?
    private external fun nativePerplexitySearch(handle: Long, intentJson: String): String?
    private external fun nativeProviderModelCatalog(handle: Long): String?
    private external fun nativeSpeechModelCatalog(handle: Long): String?
    private external fun nativeLocalModelCatalog(handle: Long): String?
    private external fun nativeByokAuthorization(intentJson: String): String?
    private external fun nativeByokExchange(handle: Long, intentJson: String): String?
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
