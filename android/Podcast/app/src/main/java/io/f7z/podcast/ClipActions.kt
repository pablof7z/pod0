package io.f7z.podcast

import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.Json

/**
 * Canonical wire contract for the `podcast.clip` kernel action namespace.
 *
 * Wire shapes verified against
 * `apps/nmp-app-podcast/src/ffi/actions/clip_module.rs`:
 *
 *  `podcast.clip.create`  — `{"op":"create","episode_id":"…","start_secs":N,"end_secs":N}`
 *                            optional: `"title":"…"`, `"source":"…"`,
 *                            `"transcript_text":"…"`, `"client_clip_id":"…"`
 *  `podcast.clip.delete`  — `{"op":"delete","clip_id":"…"}`
 *  `podcast.clip.auto_snip` — `{"op":"auto_snip","episode_id":"…","position_secs":N}`
 *                             optional: `"source":"…"`, `"client_clip_id":"…"`
 *
 * The Rust enum uses `#[serde(tag = "op", rename_all = "snake_case")]`, so:
 *  - `Create` variant → `"op":"create"`
 *  - `Delete` variant → `"op":"delete"`
 *  - `AutoSnip` variant → `"op":"auto_snip"`
 *
 * Payload builders are pure functions — no KernelBridge dependency — so they
 * can be tested without the native library loaded (same pattern as
 * [IdentityActions.buildPublishProfilePayload]).
 */
object ClipActions {
    /** Action namespace — maps to `ClipActionModule::NAMESPACE` in clip_module.rs. */
    const val NAMESPACE = "podcast.clip"

    private val json = Json

    // ── Public dispatch helpers ──────────────────────────────────────────────

    /**
     * Dispatch `podcast.clip.create` to the kernel.
     *
     * [episodeId] — kernel UUID string for the source episode.
     * [startSecs] — clip start position in seconds, absolute within the episode.
     * [endSecs]   — clip end position in seconds; must be > [startSecs]
     *               (the kernel enforces this and returns `{"ok":false}` otherwise).
     * [title]     — optional user-facing clip name; omitted from the wire payload
     *               when blank (the kernel treats absent `title` as unnamed clip).
     *
     * Returns the raw kernel JSON response string, or null on FFI failure.
     */
    fun create(
        bridge: KernelBridge,
        episodeId: String,
        startSecs: Double,
        endSecs: Double,
        title: String? = null,
    ): String? = bridge.dispatchAction(NAMESPACE, buildCreatePayload(episodeId, startSecs, endSecs, title))

    /**
     * Dispatch `podcast.clip.delete` to the kernel.
     *
     * Idempotent — the kernel returns `{"ok":true}` even when the [clipId] is
     * unknown (already deleted). This matches the Rust contract comment in
     * clip_module.rs: "idempotent delete".
     *
     * Returns the raw kernel JSON response string, or null on FFI failure.
     */
    fun delete(bridge: KernelBridge, clipId: String): String? =
        bridge.dispatchAction(NAMESPACE, buildDeletePayload(clipId))

    /**
     * Dispatch `podcast.clip.auto_snip` to the kernel.
     *
     * Creates a clip centered on [positionSecs] with a ±30-second window,
     * clamped to episode boundaries when the kernel knows the episode duration.
     *
     * Returns the raw kernel JSON response string, or null on FFI failure.
     */
    fun autoSnip(bridge: KernelBridge, episodeId: String, positionSecs: Double): String? =
        bridge.dispatchAction(NAMESPACE, buildAutoSnipPayload(episodeId, positionSecs))

    // ── Pure payload builders (testable without bridge) ──────────────────────

    /**
     * Build the `podcast.clip.create` wire payload.
     *
     * Rust contract (`ClipAction::Create`):
     * ```json
     * {"op":"create","episode_id":"<uuid>","start_secs":N,"end_secs":N}
     * ```
     * Optional field when title is non-blank:
     * ```json
     * {"op":"create","episode_id":"<uuid>","start_secs":N,"end_secs":N,"title":"…"}
     * ```
     *
     * `title` is omitted entirely when null or blank — the Rust struct uses
     * `#[serde(default, skip_serializing_if = "Option::is_none")]` on the
     * `title` field, meaning an absent title decodes as `None` (unnamed clip).
     * Sending an empty string would produce a titled clip with an empty title,
     * which is wrong.
     */
    fun buildCreatePayload(
        episodeId: String,
        startSecs: Double,
        endSecs: Double,
        title: String? = null,
    ): String {
        val fields = mutableMapOf<String, JsonElement>(
            "op"         to JsonPrimitive("create"),
            "episode_id" to JsonPrimitive(episodeId),
            "start_secs" to JsonPrimitive(startSecs),
            "end_secs"   to JsonPrimitive(endSecs),
        )
        val trimmedTitle = title?.trim()
        if (!trimmedTitle.isNullOrEmpty()) {
            fields["title"] = JsonPrimitive(trimmedTitle)
        }
        return json.encodeToString(JsonObject.serializer(), JsonObject(fields))
    }

    /**
     * Build the `podcast.clip.delete` wire payload.
     *
     * Rust contract (`ClipAction::Delete`):
     * ```json
     * {"op":"delete","clip_id":"<uuid>"}
     * ```
     */
    fun buildDeletePayload(clipId: String): String =
        json.encodeToString(
            JsonObject.serializer(),
            JsonObject(
                mapOf(
                    "op"      to JsonPrimitive("delete"),
                    "clip_id" to JsonPrimitive(clipId),
                ),
            ),
        )

    /**
     * Build the `podcast.clip.auto_snip` wire payload.
     *
     * Rust contract (`ClipAction::AutoSnip`):
     * ```json
     * {"op":"auto_snip","episode_id":"<uuid>","position_secs":N}
     * ```
     */
    fun buildAutoSnipPayload(
        episodeId: String,
        positionSecs: Double,
        source: String? = null,
        clientClipId: String? = null,
    ): String =
        json.encodeToString(
            JsonObject.serializer(),
            JsonObject(
                buildMap {
                    put("op", JsonPrimitive("auto_snip"))
                    put("episode_id", JsonPrimitive(episodeId))
                    put("position_secs", JsonPrimitive(positionSecs))
                    source?.trim()?.takeIf { it.isNotEmpty() }?.let {
                        put("source", JsonPrimitive(it))
                    }
                    clientClipId?.trim()?.takeIf { it.isNotEmpty() }?.let {
                        put("client_clip_id", JsonPrimitive(it))
                    }
                },
            ),
        )
}
