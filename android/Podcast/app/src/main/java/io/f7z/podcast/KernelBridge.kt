package io.f7z.podcast

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
     * Host → kernel capability-report channel (M13.A stub). The Kotlin
     * capability stubs in `capabilities/` call this with the namespace
     * (`"nmp.audio.capability"`, `"nmp.download.capability"`, …) and the
     * JSON-encoded report (`AudioReport::Playing`, …).
     *
     * Returns `0` on success, `-1` on input failure. Mirrors the iOS
     * `attach(sendReport:)` closure's wire shape: the host reports, the
     * kernel decides (D7).
     */
    external fun nmpCapabilityReport(namespace: String, reportJson: String): Int

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
    private external fun nativeSigninNsec(handle: Long, nsec: String)
    private external fun nativeNextUpdate(handle: Long): String?
    private external fun nativePodcastSnapshot(handle: Long): String?
    private external fun nativeFree(handle: Long)

    companion object {
        /**
         * Matches the Rust `[lib] name = "nmp_app_podcast"`. `System.loadLibrary`
         * strips the `lib` prefix and `.so` suffix, so we pass the bare name.
         */
        private const val LIB_NAME = "nmp_app_podcast"
    }
}
