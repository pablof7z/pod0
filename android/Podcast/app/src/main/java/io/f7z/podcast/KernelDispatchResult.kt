package io.f7z.podcast

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

// ─── dispatch_action return envelope ─────────────────────────────────────
//
// Mirrors Swift's `DispatchResult` enum in
// `App/Sources/Bridge/KernelBridge+Decode.swift` exactly.
//
// The Rust kernel (`ActionRegistry::start`) returns one of two JSON shapes:
//   Accepted : {"correlation_id":"<32-hex>"}
//   Rejected : {"error":"<human-readable reason>"}
//
// A null return from the FFI layer means the native call itself failed before
// the kernel could respond. Any other shape degrades to Failure so no action
// is silently lost.

/**
 * Synchronous outcome of `PodcastApp.dispatchPodcastAction`
 * (via [KernelBridge.dispatchAction]).
 *
 * The two variants mirror Swift's `DispatchResult.accepted` / `.failure`.
 * Callers must ONLY treat [Accepted] as a successful dispatch — a [Failure]
 * must surface the [Failure.message] to the user and MUST NOT advance or
 * cache any local state.
 */
sealed class DispatchResult {
    /**
     * Action accepted and enqueued by the kernel.
     * [correlationId] is the ID minted by `ActionRegistry::start`.
     */
    data class Accepted(val correlationId: String) : DispatchResult()

    /**
     * Action rejected synchronously by the kernel, or the FFI call itself
     * failed. [message] is human-readable and safe to surface directly in UI.
     */
    data class Failure(val message: String) : DispatchResult()

    companion object {
        private val json = Json

        /**
         * Parse the JSON envelope returned by `PodcastApp.dispatchPodcastAction`.
         *
         * Mirrors Swift's `DispatchResult.parse(envelope:)` exactly:
         *  - Non-empty `"correlation_id"` string → [Accepted]
         *  - `"error"` string present              → [Failure] with kernel reason
         *  - null (FFI failure)                    → [Failure] with canned message
         *  - Anything else (malformed / unknown)   → [Failure] (safe degradation)
         */
        fun parseEnvelope(raw: String?): DispatchResult {
            if (raw == null) {
                return Failure("Couldn't reach the kernel. Try again.")
            }
            return try {
                val obj = json.decodeFromString(JsonObject.serializer(), raw)
                // Accepted: has a non-empty string-typed "correlation_id".
                val corrId = (obj["correlation_id"] as? JsonPrimitive)
                    ?.takeIf { it.isString }
                    ?.content
                    ?.takeIf { it.isNotEmpty() }
                if (corrId != null) return Accepted(corrId)
                // Rejected: has a string-typed "error" field.
                val errorMsg = (obj["error"] as? JsonPrimitive)
                    ?.takeIf { it.isString }
                    ?.content
                    ?.takeIf { it.isNotEmpty() }
                Failure(
                    errorMsg
                        ?: "Dispatch envelope missing both correlation_id and error " +
                        "(bytes=${raw.length}).",
                )
            } catch (e: Exception) {
                Failure("Dispatch envelope was not valid JSON (bytes=${raw.length}).")
            }
        }
    }
}
