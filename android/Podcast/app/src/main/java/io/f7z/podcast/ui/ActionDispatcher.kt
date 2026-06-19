package io.f7z.podcast.ui

import io.f7z.podcast.KernelBridge
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

/**
 * Kernel action wire contract for the Android shell.
 *
 * **The wire model is `(namespace, op-tagged body)` — not flat action ids.**
 *
 * The Rust kernel registers podcast-domain `ActionModule` namespaces including:
 *
 *  * `"podcast"`        — subscribe, unsubscribe, refresh_all, search_itunes,
 *                         download, delete_download, star_episode, …
 *  * `"podcast.player"` — play, pause, resume, seek, set_speed,
 *                         set_sleep_timer, stop, …
 *  * `"podcast.queue"`  — add_next, add_last, remove, clear.
 *  * `"podcast.settings"` — playback preferences, provider/model metadata.
 *  * `"podcast.tasks"`  — create/delete/enable/disable/run scheduled tasks.
 *
 * Each action is encoded as `{"op":"<variant>", …fields}` and the Rust
 * `#[serde(tag = "op", rename_all = "snake_case")]` discriminator routes it.
 * `KernelBridge.dispatchAction(namespace, body)` → `nmp_app_dispatch_action`
 * forwards `(namespace, body)` straight into that registry, exactly like the
 * iOS `kernel.dispatch(namespace:body:)` reference in
 * `App/Sources/Bridge/AppStateStore+KernelActions.swift`.
 *
 * Source of truth verified against:
 *  * `apps/nmp-app-podcast/src/ffi/actions/podcast_module.rs` (PodcastAction)
 *  * `apps/nmp-app-podcast/src/ffi/actions/player_module.rs`  (PlayerAction)
 *  * `apps/nmp-app-podcast/src/android.rs` (nativeDispatchAction)
 *
 * NOTE: an earlier demo passed the *dotted op path* (e.g. `"podcast.player.play"`)
 * as the namespace argument. That string is not a registered namespace, so
 * those dispatches never reached the kernel. This file fixes the contract;
 * payloads carry their `op` as a defaulted field so `encodeDefaults = true`
 * emits the discriminator.
 */
object PodcastNamespace {
    const val PODCAST = "podcast"
    const val PLAYER = "podcast.player"
    const val QUEUE = "podcast.queue"
    const val SETTINGS = "podcast.settings"
    const val TASKS = "podcast.tasks"
    const val INBOX = "podcast.inbox"
    const val AGENT = "podcast.agent"
    const val PICKS = "podcast.picks"
    /** AI chapter synthesis namespace. Verified against `ChaptersActionModule::NAMESPACE`. */
    const val CHAPTERS = "podcast.chapters"
}

// ── `podcast` namespace payloads ──────────────────────────────────────────
//
// Every payload pins its `op` discriminator as a defaulted field so callers
// never pass it by hand and `Json { encodeDefaults = true }` serializes it.

@Serializable
data class SubscribePayload(
    @SerialName("feed_url") val feedUrl: String,
    val op: String = "subscribe",
)

/**
 * Remove only the follow membership, keeping the podcast row + episodes as
 * "known but unfollowed". Mirrors iOS `kernelUnfollow` / Rust `podcast.unfollow`
 * (`PodcastAction::Unfollow { podcast_id }`). A subsequent re-subscribe with
 * the same feed URL is instant — no network fetch needed.
 *
 * Wire this to any Android "Unsubscribe" affordance: in this app "Unsubscribe"
 * keeps history. A permanent hard-delete (the Rust `podcast.unsubscribe` action,
 * which removes the podcast + episodes) must be a separately-labeled "Delete"
 * action with its own wire type, added when Android grows library-management UI.
 *
 * Backlog: android-unfollow-parity (#547/#573) — no Android unsubscribe/delete
 * UI exists yet; add the unfollow dispatch (and a delete wire type) when added.
 */
@Serializable
data class UnfollowPayload(
    @SerialName("podcast_id") val podcastId: String,
    val op: String = "unfollow",
)

@Serializable
data class SearchPayload(
    val query: String,
    val op: String = "search_itunes",
)

@Serializable
data class RefreshAllPayload(val op: String = "refresh_all")

@Serializable
data class DownloadStartPayload(
    @SerialName("episode_id") val episodeId: String,
    val op: String = "download",
)

/**
 * Remove a previously-**completed** download from disk and clear the kernel's
 * `local_path` mapping. Verified against `PodcastAction::DeleteDownload` →
 * `handle_delete_download`, which only `clear_local_path`s + `remove_file`s.
 *
 * NOTE: this does **not** cancel an in-flight download — it operates on the
 * store's `local_path`, which is only stamped on completion. To cancel an
 * active/queued/paused download use [CancelDownloadPayload] on the
 * `podcast.player` namespace instead.
 */
@Serializable
data class DownloadDeletePayload(
    @SerialName("episode_id") val episodeId: String,
    val op: String = "delete_download",
)

@Serializable
data class FetchFeedbackPayload(val op: String = "fetch_feedback")

@Serializable
data class PublishFeedbackPayload(
    val category: String,
    val content: String,
    @SerialName("parent_event_id") val parentEventId: String? = null,
    @SerialName("reply_to_pubkey") val replyToPubkey: String? = null,
    val op: String = "publish_feedback",
)

// ── `podcast.player` namespace payloads ───────────────────────────────────

/**
 * Cancel an active, paused, or queued download. Verified against
 * `PlayerAction::CancelDownload` → `q.cancel(&episode_id)`. This is the
 * correct op for the "cancel" affordance on an *active* download row —
 * `delete_download` (podcast namespace) only removes a finished file.
 */
@Serializable
data class CancelDownloadPayload(
    @SerialName("episode_id") val episodeId: String,
    val op: String = "cancel_download",
)

@Serializable
data class PlayPayload(
    @SerialName("episode_id") val episodeId: String,
    val op: String = "play",
)

@Serializable
data class PausePayload(val op: String = "pause")

@Serializable
data class SeekPayload(
    @SerialName("position_secs") val positionSecs: Double,
    val op: String = "seek",
)

@Serializable
data class SetSpeedPayload(
    val speed: Float,
    val op: String = "set_speed",
)

/**
 * Arm (`secs = N`) or clear (`secs = null`) the sleep timer. Field is `secs`
 * (not `seconds`) per `PlayerAction::SetSleepTimer { secs: Option<u64> }`.
 */
@Serializable
data class SleepTimerPayload(
    val secs: Int?,
    val op: String = "set_sleep_timer",
)

@Serializable
data class PlayNextPayload(val op: String = "play_next")

// ── `podcast.queue` namespace payloads ────────────────────────────────────

@Serializable
data class QueueAddNextPayload(
    @SerialName("episode_id") val episodeId: String,
    val op: String = "add_next",
)

@Serializable
data class QueueAddLastPayload(
    @SerialName("episode_id") val episodeId: String,
    val op: String = "add_last",
)

@Serializable
data class QueueRemovePayload(
    @SerialName("episode_id") val episodeId: String,
    val op: String = "remove",
)

@Serializable
data class QueueClearPayload(val op: String = "clear")

// ── `podcast.tasks` namespace payloads ────────────────────────────────────

@Serializable
sealed class AgentTaskIntentPayload {
    @Serializable
    @SerialName("inbox_triage")
    object InboxTriage : AgentTaskIntentPayload()

    @Serializable
    @SerialName("clear_agent")
    object ClearAgent : AgentTaskIntentPayload()

    @Serializable
    @SerialName("remember_memory")
    data class RememberMemory(
        val key: String,
        val value: String,
    ) : AgentTaskIntentPayload()

    @Serializable
    @SerialName("agent_prompt")
    data class AgentPrompt(
        val prompt: String,
    ) : AgentTaskIntentPayload()

    companion object {
        fun inboxTriage(): AgentTaskIntentPayload =
            InboxTriage

        fun clearAgent(): AgentTaskIntentPayload =
            ClearAgent

        fun rememberMemory(key: String, value: String): AgentTaskIntentPayload =
            RememberMemory(key = key, value = value)

        fun agentPrompt(prompt: String): AgentTaskIntentPayload =
            AgentPrompt(prompt = prompt)
    }
}

@Serializable
data class TaskCreateFromIntentPayload(
    val title: String,
    val description: String? = null,
    val intent: AgentTaskIntentPayload,
    val schedule: String,
    val op: String = "create_from_intent",
)

@Serializable
data class TaskUpdateFromIntentPayload(
    @SerialName("task_id") val taskId: String,
    val title: String,
    val description: String? = null,
    val intent: AgentTaskIntentPayload,
    val schedule: String,
    val op: String = "update_from_intent",
)

@Serializable
data class TaskDeletePayload(
    @SerialName("task_id") val taskId: String,
    val op: String = "delete",
)

@Serializable
data class TaskEnablePayload(
    @SerialName("task_id") val taskId: String,
    val op: String = "enable",
)

@Serializable
data class TaskDisablePayload(
    @SerialName("task_id") val taskId: String,
    val op: String = "disable",
)

@Serializable
data class TaskRunDuePayload(val op: String = "run_due")

@Serializable
data class TaskRunNowPayload(
    @SerialName("task_id") val taskId: String,
    val op: String = "run_now",
)

// ── `podcast.settings` namespace payloads ─────────────────────────────────

/**
 * Set the default playback rate. Verified against
 * `SettingsAction::SetDefaultPlaybackRate { rate: f64 }` — the field is
 * `rate` (NOT the task-spec's `set_default_speed`/`rate` on `podcast`).
 * The kernel clamps server-side to `[0.5, 3.0]`.
 */
@Serializable
data class SetDefaultPlaybackRatePayload(
    val rate: Double,
    val op: String = "set_default_playback_rate",
)

/**
 * Toggle delete-downloaded-file-after-played. Verified against
 * `SettingsAction::SetAutoDeleteDownloadsAfterPlayed { enabled: bool }`.
 */
@Serializable
data class SetAutoDeleteDownloadsPayload(
    val enabled: Boolean,
    val op: String = "set_auto_delete_downloads_after_played",
)

@Serializable
data class SetAgentInitialModelPayload(
    val model: String,
    @SerialName("model_name") val modelName: String,
    val op: String = "set_agent_initial_model",
)

@Serializable
data class SetAgentThinkingModelPayload(
    val model: String,
    @SerialName("model_name") val modelName: String,
    val op: String = "set_agent_thinking_model",
)

@Serializable
data class SetMemoryCompilationModelPayload(
    val model: String,
    @SerialName("model_name") val modelName: String,
    val op: String = "set_memory_compilation_model",
)

@Serializable
data class SetCategorizationModelPayload(
    val model: String,
    @SerialName("model_name") val modelName: String,
    val op: String = "set_categorization_model",
)

@Serializable
data class SetChapterCompilationModelPayload(
    val model: String,
    @SerialName("model_name") val modelName: String,
    val op: String = "set_chapter_compilation_model",
)

@Serializable
data class SetEmbeddingsModelPayload(
    val model: String,
    @SerialName("model_name") val modelName: String,
    val op: String = "set_embeddings_model",
)

@Serializable
data class SetImageGenerationModelPayload(
    val model: String,
    @SerialName("model_name") val modelName: String,
    val op: String = "set_image_generation_model",
)

@Serializable
data class SetRerankerEnabledPayload(
    val enabled: Boolean,
    val op: String = "set_reranker_enabled",
)

@Serializable
data class SetOpenRouterCredentialPayload(
    val source: String,
    @SerialName("key_id") val keyId: String? = null,
    @SerialName("key_label") val keyLabel: String? = null,
    // connected_at removed: kernel stamps time (D9); field is authoritative in
    // the projection but must not be supplied in the action payload.
    val op: String = "set_open_router_credential",
)

@Serializable
data class SetOllamaCredentialPayload(
    val source: String,
    @SerialName("key_id") val keyId: String? = null,
    @SerialName("key_label") val keyLabel: String? = null,
    // connected_at removed: kernel stamps time (D9).
    val op: String = "set_ollama_credential",
)

@Serializable
data class SetOllamaChatUrlPayload(
    val url: String,
    val op: String = "set_ollama_chat_url",
)

@Serializable
data class SetElevenLabsCredentialPayload(
    val source: String,
    @SerialName("key_id") val keyId: String? = null,
    @SerialName("key_label") val keyLabel: String? = null,
    // connected_at removed: kernel stamps time (D9).
    val op: String = "set_eleven_labs_credential",
)

@Serializable
data class SetAssemblyAiCredentialPayload(
    val source: String,
    @SerialName("key_id") val keyId: String? = null,
    @SerialName("key_label") val keyLabel: String? = null,
    // connected_at removed: kernel stamps time (D9).
    val op: String = "set_assembly_ai_credential",
)

@Serializable
data class SetPerplexityCredentialPayload(
    val source: String,
    @SerialName("key_id") val keyId: String? = null,
    @SerialName("key_label") val keyLabel: String? = null,
    // connected_at removed: kernel stamps time (D9).
    val op: String = "set_perplexity_credential",
)

@Serializable
data class SetSttProviderPayload(
    val provider: String,
    val op: String = "set_stt_provider",
)

@Serializable
data class SetSttKeysPresentPayload(
    val providers: List<String>,
    val op: String = "set_stt_keys_present",
)

@Serializable
data class SetOpenRouterWhisperModelPayload(
    val model: String,
    val op: String = "set_open_router_whisper_model",
)

@Serializable
data class SetAssemblyAiSttModelPayload(
    val model: String,
    val op: String = "set_assembly_ai_stt_model",
)

@Serializable
data class SetElevenLabsModelsPayload(
    @SerialName("stt_model") val sttModel: String,
    @SerialName("tts_model") val ttsModel: String,
    val op: String = "set_eleven_labs_models",
)

@Serializable
data class SetElevenLabsVoicePayload(
    @SerialName("voice_id") val voiceId: String,
    @SerialName("voice_name") val voiceName: String,
    val op: String = "set_eleven_labs_voice",
)

/**
 * Load provider secrets into the Rust store's in-memory provider cache.
 * Secrets are intentionally not projected and must not be persisted by the
 * kernel; the Android host owns secure storage.
 */
@Serializable
data class SetProviderApiKeysPayload(
    @SerialName("open_router") val openRouter: String? = null,
    val ollama: String? = null,
    @SerialName("eleven_labs") val elevenLabs: String? = null,
    @SerialName("assembly_ai") val assemblyAi: String? = null,
    val perplexity: String? = null,
    val op: String = "set_provider_api_keys",
)

// Inbox + transcript payloads live in InboxTranscriptPayloads.kt (file-length split per AGENTS.md).

// ── `podcast.agent` namespace ─────────────────────────────────────────────
//
// Verified against `apps/nmp-app-podcast/src/ffi/actions/agent_module.rs`:
//
//   AgentChatAction::Send  { message: String }  → op = "send"
//   AgentChatAction::Clear                       → op = "clear"
//
// `#[serde(tag = "op", rename_all = "snake_case")]` maps variant names to
// the snake_case `op` string. `Send { message }` carries one extra field.

// ── `podcast.picks` namespace payloads ───────────────────────────────────
//
// Verified against `apps/nmp-app-podcast/src/ffi/actions/picks_module.rs`:
//
//   PicksAction::Refresh  →  op = "refresh"
//
// `#[serde(tag = "op", rename_all = "snake_case")]` maps the variant name.

/**
 * Trigger a synchronous picks recompute. The handler walks the library with
 * the heuristic (newest-first, per-show cap) and, if Ollama is available,
 * follows up with the LLM scoring pass. The updated `Vec<AgentPickSummary>`
 * is stamped onto `PodcastHandle` and rides the next `podcast.misc` push frame.
 */
@Serializable
data class PicksRefreshPayload(val op: String = "refresh")

@Serializable
data class AgentSendPayload(
    val message: String,
    val op: String = "send",
)

@Serializable
data class AgentClearPayload(val op: String = "clear")

// AI feature payloads (podcast.chapters + podcast.settings ad-skip) live in
// AiFeaturePayloads.kt to keep this file under the 500-line hard limit.

/**
 * Thin wrapper around `KernelBridge.dispatchAction`. Encodes a typed,
 * op-tagged payload to JSON (one place, one config) and forwards it to the
 * given namespace. No business logic, no state (D5/D8) — the caller picks the
 * namespace + payload; the kernel decides the outcome and reports it on the
 * next snapshot.
 */
object PodcastActionDispatcher {
    @PublishedApi
    internal val json: Json = Json { encodeDefaults = true; ignoreUnknownKeys = true }

    /** Dispatch a typed, op-tagged payload to [namespace]. Returns the envelope JSON. */
    inline fun <reified T> dispatch(
        bridge: KernelBridge,
        namespace: String,
        payload: T,
    ): String? = bridge.dispatchAction(namespace, json.encodeToString<T>(payload))
}
