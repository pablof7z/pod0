package io.f7z.podcast

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/**
 * Mirror of `apps/nmp-app-podcast/src/ffi/projections.rs::DownloadQueueSnapshot`.
 */
@Serializable
data class DownloadQueueSnapshot(
    val active: List<DownloadItemSnapshot> = emptyList(),
    @SerialName("queued_count") val queuedCount: Int = 0,
    @SerialName("completed_today") val completedToday: Int = 0,
)

/**
 * Mirror of `DownloadItemSnapshot`. `state` is a string discriminator
 * (`"active"` / `"queued"` / `"paused"` / `"failed"`).
 */
@Serializable
data class DownloadItemSnapshot(
    @SerialName("episode_id") val episodeId: String,
    /**
     * Enclosure URL the executor fetches. Projected by Rust
     * (`ffi/projections/download.rs::DownloadItemSnapshot.url`) so the
     * pull-model `DownloadCapability` can start the HTTP download straight
     * from a `"queued"` / `"active"` row while the kernel remains the only
     * download policy owner. Empty when the row predates the field.
     */
    val url: String = "",
    val progress: Float = 0.0f,
    val state: String,
    /**
     * Total file size in bytes once the server reports `Content-Length`.
     * `null` until the first HTTP response. Mirror of the Rust
     * `DownloadItemSnapshot.total_bytes` (`Option<u64>`); only ever present
     * for in-flight rows — completed downloads drop out of `active` entirely.
     */
    @SerialName("total_bytes") val totalBytes: Long? = null,
    val error: String? = null,
)

/**
 * Android mirror of the settings fields currently consumed by playback and
 * provider/model configuration surfaces.
 */
@Serializable
data class SettingsSnapshot(
    @SerialName("default_playback_rate") val defaultPlaybackRate: Float = 1.0f,
    @SerialName("auto_delete_downloads_after_played") val autoDeleteDownloads: Boolean = false,
    /**
     * Whether the player actor automatically seeks past detected ad segments.
     * Mirror of `SettingsSnapshot.auto_skip_ads_enabled` (Rust field name).
     * Toggled via `podcast.settings` `{"op":"set_auto_skip_ads","enabled":b}`.
     */
    @SerialName("auto_skip_ads_enabled") val autoSkipAdsEnabled: Boolean = false,
    @SerialName("agent_initial_model") val agentInitialModel: String = "deepseek-v4-flash:cloud",
    @SerialName("agent_initial_model_name") val agentInitialModelName: String = "DeepSeek Flash",
    @SerialName("agent_thinking_model") val agentThinkingModel: String = "deepseek-v4-pro:cloud",
    @SerialName("agent_thinking_model_name") val agentThinkingModelName: String = "DeepSeek Pro",
    @SerialName("memory_compilation_model") val memoryCompilationModel: String = "deepseek-v4-flash:cloud",
    @SerialName("memory_compilation_model_name") val memoryCompilationModelName: String = "DeepSeek Flash",
    @SerialName("categorization_model") val categorizationModel: String = "deepseek-v4-flash:cloud",
    @SerialName("categorization_model_name") val categorizationModelName: String = "DeepSeek Flash",
    @SerialName("chapter_compilation_model") val chapterCompilationModel: String = "deepseek-v4-flash:cloud",
    @SerialName("chapter_compilation_model_name") val chapterCompilationModelName: String = "DeepSeek Flash",
    @SerialName("embeddings_model") val embeddingsModel: String = "deepseek-v4-flash:cloud",
    @SerialName("embeddings_model_name") val embeddingsModelName: String = "DeepSeek Flash",
    @SerialName("image_generation_model") val imageGenerationModel: String = "google/gemini-2.5-flash-image",
    @SerialName("image_generation_model_name") val imageGenerationModelName: String = "Gemini 2.5 Flash",
    @SerialName("reranker_enabled") val rerankerEnabled: Boolean = false,
    @SerialName("open_router_credential_source") val openRouterCredentialSource: String = "",
    @SerialName("open_router_key_present") val openRouterKeyPresent: Boolean = false,
    @SerialName("open_router_byok_key_id") val openRouterByokKeyId: String? = null,
    @SerialName("open_router_byok_key_label") val openRouterByokKeyLabel: String? = null,
    @SerialName("open_router_connected_at") val openRouterConnectedAt: Long? = null,
    @SerialName("ollama_credential_source") val ollamaCredentialSource: String = "",
    @SerialName("ollama_key_present") val ollamaKeyPresent: Boolean = false,
    @SerialName("ollama_byok_key_id") val ollamaByokKeyId: String? = null,
    @SerialName("ollama_byok_key_label") val ollamaByokKeyLabel: String? = null,
    @SerialName("ollama_connected_at") val ollamaConnectedAt: Long? = null,
    @SerialName("ollama_chat_url") val ollamaChatUrl: String = "https://ollama.com/api/chat",
    @SerialName("eleven_labs_credential_source") val elevenLabsCredentialSource: String = "",
    @SerialName("eleven_labs_key_present") val elevenLabsKeyPresent: Boolean = false,
    @SerialName("eleven_labs_byok_key_id") val elevenLabsByokKeyId: String? = null,
    @SerialName("eleven_labs_byok_key_label") val elevenLabsByokKeyLabel: String? = null,
    @SerialName("eleven_labs_connected_at") val elevenLabsConnectedAt: Long? = null,
    @SerialName("assembly_ai_credential_source") val assemblyAiCredentialSource: String = "",
    @SerialName("assembly_ai_key_present") val assemblyAiKeyPresent: Boolean = false,
    @SerialName("assembly_ai_byok_key_id") val assemblyAiByokKeyId: String? = null,
    @SerialName("assembly_ai_byok_key_label") val assemblyAiByokKeyLabel: String? = null,
    @SerialName("assembly_ai_connected_at") val assemblyAiConnectedAt: Long? = null,
    @SerialName("perplexity_credential_source") val perplexityCredentialSource: String = "",
    @SerialName("perplexity_key_present") val perplexityKeyPresent: Boolean = false,
    @SerialName("perplexity_byok_key_id") val perplexityByokKeyId: String? = null,
    @SerialName("perplexity_byok_key_label") val perplexityByokKeyLabel: String? = null,
    @SerialName("perplexity_connected_at") val perplexityConnectedAt: Long? = null,
    @SerialName("stt_provider") val sttProvider: String = "apple_native",
    @SerialName("effective_stt_provider") val effectiveSttProvider: String = "apple_native",
    @SerialName("effective_stt_provider_requires_key") val effectiveSttProviderRequiresKey: Boolean = false,
    @SerialName("open_router_whisper_model") val openRouterWhisperModel: String = "openai/whisper-1",
    @SerialName("assembly_ai_stt_model") val assemblyAiSttModel: String = "universal-3-pro,universal-2",
    @SerialName("eleven_labs_stt_model") val elevenLabsSttModel: String = "scribe_v1",
    @SerialName("eleven_labs_tts_model") val elevenLabsTtsModel: String = "eleven_turbo_v2_5",
    @SerialName("eleven_labs_voice_id") val elevenLabsVoiceId: String = "",
    @SerialName("eleven_labs_voice_name") val elevenLabsVoiceName: String = "",
)

/**
 * One AI-triaged inbox row. Mirror of `ffi/projections/inbox.rs::InboxItem`.
 *
 * Field names follow the verified Rust projection — `episode_id`,
 * `episode_title`, `podcast_id`, `podcast_title`, `priority_score`,
 * `priority_reason` — **not** the task-spec's approximated `id`/`decision`/
 * `is_hero`/`score` shape (which would silently decode to empty).
 */
@Serializable
data class InboxItem(
    @SerialName("episode_id") val episodeId: String = "",
    @SerialName("episode_title") val episodeTitle: String = "",
    @SerialName("podcast_id") val podcastId: String = "",
    @SerialName("podcast_title") val podcastTitle: String = "",
    @SerialName("artwork_url") val artworkUrl: String? = null,
    @SerialName("published_at") val publishedAt: Long = 0,
    @SerialName("duration_secs") val durationSecs: Double? = null,
    @SerialName("priority_score") val priorityScore: Float = 0.0f,
    @SerialName("priority_reason") val priorityReason: String? = null,
    @SerialName("ai_categories") val aiCategories: List<String> = emptyList(),
)

/**
 * One user-saved audio clip row. Mirror of
 * `apps/nmp-app-podcast/src/ffi/projections/clips.rs::ClipSummary`.
 *
 * All snake_case wire keys are load-bearing — kotlinx does NOT auto-convert
 * without an explicit strategy; `@SerialName` is required for every snake_case
 * field (see FFI decode contract note in [DomainFrames.kt]).
 *
 * Fields:
 *  - `id`              — kernel-assigned UUID string for the clip.
 *  - `episode_id`      — UUID of the source episode.
 *  - `episode_title`   — pre-resolved episode title (joined at projection time).
 *  - `podcast_title`   — pre-resolved podcast title.
 *  - `start_secs`      — clip start position, absolute seconds within episode.
 *  - `end_secs`        — clip end position, must be > start_secs.
 *  - `title`           — optional user-given clip title; null when unnamed.
 *  - `created_at`      — Unix seconds when the clip was created.
 *
 * Wire shape verified against Rust `ClipSummary` in
 * `apps/nmp-app-podcast/src/ffi/projections/clips.rs`.
 */
@Serializable
data class ClipSummary(
    val id: String = "",
    @SerialName("episode_id")    val episodeId: String = "",
    @SerialName("episode_title") val episodeTitle: String = "",
    @SerialName("podcast_title") val podcastTitle: String = "",
    @SerialName("start_secs")   val startSecs: Double = 0.0,
    @SerialName("end_secs")     val endSecs: Double = 0.0,
    val title: String? = null,
    @SerialName("transcript_text") val transcriptText: String = "",
    val speaker: String? = null,
    val source: String = "",
    @SerialName("refinement_status") val refinementStatus: String = "",
    @SerialName("created_at")   val createdAt: Long = 0L,
)
