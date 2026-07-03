package io.f7z.podcast

/**
 * Single-method interface for dispatching namespace-keyed actions to the
 * Rust kernel's `podcast.player` (and other) action dispatchers.
 *
 * Extracted from [KernelBridge] so that [io.f7z.podcast.capabilities.KernelForwardingPlayer]
 * can accept a test double without instantiating the native [KernelBridge]
 * (whose `init` block loads the Rust `.so` via `System.loadLibrary`).
 *
 * Production code: [KernelBridge] implements this interface. The existing
 * `ExoPlayerCapability.bridge: KernelBridge` assignment to
 * `KernelForwardingPlayer.bridge: KernelDispatcher?` remains valid because
 * [KernelBridge] is a subtype of [KernelDispatcher].
 */
fun interface KernelDispatcher {
    /**
     * Dispatch a namespace-keyed action to the Rust kernel. Returns the
     * kernel's JSON envelope or `null` on any FFI failure (D6).
     *
     * @param namespace  The actor namespace, e.g. `"podcast.player"`.
     * @param payloadJson  JSON-encoded action payload.
     */
    fun dispatchAction(namespace: String, payloadJson: String): String?
}
