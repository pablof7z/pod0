package io.f7z.podcast.capabilities

import io.f7z.podcast.KernelBridge
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

/**
 * Stub implementation of the `nmp.audio.capability` executor — Android
 * counterpart to `ios/Podcast/Podcast/Capabilities/AudioCapability.swift`,
 * but without any real playback.
 *
 * **Scope (M13.A):**
 *  * Parse `AudioCommand` (`Load` / `Play` / `Pause` / `Seek` / `Stop` /
 *    `SetVolume` / `SetSpeed` / `SetSleepTimer`) and echo back the
 *    corresponding `AudioReport` (`Playing` / `Paused` / `Stopped`) over
 *    `KernelBridge.nmpCapabilityReport(namespace, reportJson)`.
 *  * No AVFoundation, no ExoPlayer — that wiring lands in M13.B alongside
 *    the kernel-side capability router.
 *
 * **Doctrine:**
 *  * D0 — no business logic. The stub mirrors the iOS executor's wire
 *    vocabulary exactly; routing decisions live in `crate::player`.
 *  * D6 — every entry point degrades silently. `handleCommand` returns
 *    `Unit`; malformed JSON is dropped on the floor.
 *  * D7 — the stub reports, never decides. `Load` immediately reports
 *    `Playing` (mirroring AVFoundation's optimistic "we'll buffer"); the
 *    kernel decides what that means.
 */
class AudioCapabilityStub(private val bridge: KernelBridge) {

    private val json = Json {
        ignoreUnknownKeys = true
        // The stub doesn't need lenient parsing — the wire format is
        // controlled by `apps/nmp-app-podcast/src/capability/audio.rs`.
    }

    /**
     * Process one capability command and emit the synthetic report.
     *
     * The command envelope is the `AudioCommand` enum's `serde`-tagged
     * form: `{"type":"play"}`, `{"type":"load","url":"…","position_secs":…}`,
     * etc. See `crate::capability::audio::AudioCommand`.
     *
     * Returns `Unit`. Failures (parse errors, FFI rejection) are silent
     * per D6 — the kernel notices via the missing report.
     */
    fun handleCommand(commandJson: String) {
        val envelope = runCatching { json.parseToJsonElement(commandJson).jsonObject }
            .getOrNull() ?: return
        val type = envelope["type"]?.jsonPrimitive?.content ?: return

        val report: JsonObject? = when (type) {
            "load" -> {
                val url = envelope["url"]?.jsonPrimitive?.content.orEmpty()
                val position = envelope["position_secs"]?.jsonPrimitive?.content?.toDoubleOrNull() ?: 0.0
                // Stub: a real executor would buffer here. We report
                // `Playing` immediately so the kernel sees the round trip.
                buildPlayingReport(url, position, durationSecs = 0.0)
            }
            "play" -> buildPlayingReport(url = currentUrl, currentPositionSecs, durationSecs = 0.0)
            "pause" -> buildPausedReport(currentUrl, currentPositionSecs)
            "seek" -> {
                val pos = envelope["position_secs"]?.jsonPrimitive?.content?.toDoubleOrNull() ?: currentPositionSecs
                currentPositionSecs = pos
                buildPlayingReport(currentUrl, pos, durationSecs = 0.0)
            }
            "stop" -> {
                currentUrl = ""
                currentPositionSecs = 0.0
                buildJsonObject {
                    put("type", JsonPrimitive("stopped"))
                }
            }
            "set_volume", "set_speed", "set_sleep_timer" -> {
                // These are configuration commands; the canonical
                // executor doesn't echo them as reports until the
                // underlying engine confirms. Stub: no-op.
                null
            }
            else -> null
        }

        report?.let { emit(it) }
    }

    private fun buildPlayingReport(url: String, positionSecs: Double, durationSecs: Double): JsonObject {
        currentUrl = url
        currentPositionSecs = positionSecs
        return buildJsonObject {
            put("type", JsonPrimitive("playing"))
            put("url", JsonPrimitive(url))
            put("position_secs", JsonPrimitive(positionSecs))
            put("duration_secs", JsonPrimitive(durationSecs))
        }
    }

    private fun buildPausedReport(url: String, positionSecs: Double): JsonObject {
        currentUrl = url
        currentPositionSecs = positionSecs
        return buildJsonObject {
            put("type", JsonPrimitive("paused"))
            put("url", JsonPrimitive(url))
            put("position_secs", JsonPrimitive(positionSecs))
        }
    }

    private fun emit(report: JsonObject) {
        val payload = json.encodeToString(JsonObject.serializer(), report)
        bridge.nmpCapabilityReport(NAMESPACE, payload)
    }

    // ─── Local synthetic state ──────────────────────────────────────────
    //
    // The stub keeps just enough to answer pause/seek after a play. A real
    // executor would derive these from the underlying audio engine — D8
    // bounded reactivity still applies (≤4 Hz position reports) when the
    // M13.B wiring lands.

    private var currentUrl: String = ""
    private var currentPositionSecs: Double = 0.0

    companion object {
        /**
         * Matches `apps/nmp-app-podcast/src/capability/audio.rs::AUDIO_CAPABILITY_NAMESPACE`.
         * If the Rust constant moves, this must move with it.
         */
        const val NAMESPACE: String = "nmp.audio.capability"
    }
}
