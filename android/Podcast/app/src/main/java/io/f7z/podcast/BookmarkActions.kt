package io.f7z.podcast

import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.Json

/**
 * Canonical wire contract for the `podcast` namespace's `StarEpisode` action.
 *
 * Wire shape verified against
 * `apps/nmp-app-podcast/src/ffi/actions/podcast_module.rs::PodcastAction::StarEpisode`:
 *
 * ```rust
 * StarEpisode {
 *     episode_id: String,
 *     #[serde(default, skip_serializing_if = "Option::is_none")]
 *     starred: Option<bool>,
 * }
 * ```
 *
 * The enum uses `#[serde(tag = "op", rename_all = "snake_case")]`, so the variant
 * `StarEpisode` serialises as `"op":"star_episode"`.
 *
 * Namespace: `"podcast"` (`PodcastActionModule::NAMESPACE` in podcast_module.rs).
 *
 * When `starred` is omitted the kernel **flips** the current value (toggle);
 * when `starred` is `true`/`false` it sets it explicitly. The updated flag
 * surfaces on the next snapshot tick via `EpisodeSummary.starred`.
 *
 * Payload builders are pure functions — no KernelBridge dependency — so they
 * can be tested without the native library loaded (same pattern as
 * [ClipActions.buildCreatePayload]).
 */
object BookmarkActions {
    /** Action namespace — matches `PodcastActionModule::NAMESPACE = "podcast"`. */
    const val NAMESPACE = "podcast"

    private val json = Json

    // ── Public dispatch helpers ──────────────────────────────────────────────

    /**
     * Toggle the starred flag on [episodeId].
     *
     * Passes `starred = null` to the kernel, which flips the current value.
     * The updated state surfaces on the next snapshot tick via
     * `EpisodeSummary.starred`. Returns the raw kernel JSON response, or null
     * on FFI failure (D6).
     */
    fun toggle(bridge: KernelBridge, episodeId: String): String? =
        bridge.dispatchAction(NAMESPACE, buildTogglePayload(episodeId))

    /**
     * Explicitly set the starred flag on [episodeId].
     *
     * Passes `starred = [starred]` to the kernel so the action is idempotent
     * for known-state callers (e.g. "unstar from list" always sends `false`).
     * Returns the raw kernel JSON response, or null on FFI failure (D6).
     */
    fun setStar(bridge: KernelBridge, episodeId: String, starred: Boolean): String? =
        bridge.dispatchAction(NAMESPACE, buildSetStarPayload(episodeId, starred))

    // ── Pure payload builders (testable without bridge) ──────────────────────

    /**
     * Build the `star_episode` toggle payload (no explicit `starred` field).
     *
     * Rust contract (`PodcastAction::StarEpisode { starred: None }`):
     * ```json
     * {"op":"star_episode","episode_id":"<uuid>"}
     * ```
     *
     * `starred` is omitted when `None` per
     * `#[serde(default, skip_serializing_if = "Option::is_none")]`.
     */
    fun buildTogglePayload(episodeId: String): String =
        json.encodeToString(
            JsonObject.serializer(),
            JsonObject(
                mapOf(
                    "op"         to JsonPrimitive("star_episode"),
                    "episode_id" to JsonPrimitive(episodeId),
                ),
            ),
        )

    /**
     * Build the `star_episode` explicit-set payload.
     *
     * Rust contract (`PodcastAction::StarEpisode { starred: Some(bool) }`):
     * ```json
     * {"op":"star_episode","episode_id":"<uuid>","starred":true}
     * ```
     */
    fun buildSetStarPayload(episodeId: String, starred: Boolean): String =
        json.encodeToString(
            JsonObject.serializer(),
            JsonObject(
                mapOf(
                    "op"         to JsonPrimitive("star_episode"),
                    "episode_id" to JsonPrimitive(episodeId),
                    "starred"    to JsonPrimitive(starred),
                ),
            ),
        )
}
