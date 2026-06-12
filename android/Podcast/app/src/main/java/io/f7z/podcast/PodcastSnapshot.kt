package io.f7z.podcast

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement

/**
 * Kotlin mirror of `apps/nmp-app-podcast/src/ffi/snapshot.rs::PodcastUpdate`.
 *
 * Every field on the Rust struct has a matching property here so the Compose
 * shell can render any state the kernel projects. New fields land on both
 * sides simultaneously. The canonical wire shape lives in
 * `apps/nmp-app-podcast/src/ffi/snapshot.rs`.
 *
 * Every field below this line is optional / defaulted so the existing payload
 * still decodes. As later milestones (M1, M2.A, M3.A, M9.A, …) extend
 * `PodcastUpdate` in Rust, the matching field on this struct starts carrying
 * real data with **zero** Kotlin-side changes.
 *
 * The `Json` decoder is configured with `ignoreUnknownKeys = true` so an older
 * Android build can still decode a newer kernel snapshot (forward compat).
 *
 * Wire-shape source of truth: `apps/nmp-app-podcast/src/ffi/snapshot.rs`
 * (`PodcastUpdate`) + `apps/nmp-app-podcast/src/ffi/projections.rs`.
 *
 * **Doctrine — D5 / D7:**
 *  * The kernel decides what to surface; this struct is pure decode +
 *    render scaffolding. No Kotlin-side derivations beyond `null` checks.
 *  * `Option<T>` on the Rust side becomes nullable here, with `null`
 *    defaults so missing JSON fields decode cleanly (forward compat).
 */
@Serializable
data class PodcastSnapshot(
    val running: Boolean = false,
    val rev: Long = 0,
    @SerialName("schema_version") val schemaVersion: Int = 0,
    /** Active player projection, `null` when no episode is loaded. */
    @SerialName("now_playing") val nowPlaying: NowPlayingState? = null,
    /** Active download queue, `null` until the first enqueue. */
    val downloads: DownloadQueueSnapshot? = null,
    /** Agent-chat projection, `null` until the first turn. */
    val agent: AgentSnapshot? = null,
    /** Briefing scheduler state, `null` until first scheduler touch. */
    val briefing: BriefingSnapshot? = null,
    /** Voice/TTS session state, `null` while idle. */
    val voice: VoiceStateSnapshot? = null,
    /** Widget/Live-Activity projection, `null` until populated. */
    val widget: WidgetSnapshot? = null,
    /** Transient toast the kernel wants the host to surface, or `null`. */
    val toast: String? = null,
    /** Active identity (M1.A — `active_account` snapshot field). `null` when nobody is signed in. */
    @SerialName("active_account") val activeAccount: AccountSummary? = null,
    /**
     * Library rows. Emitted by the kernel under the `library` wire key today
     * (M2.F stub) and will migrate to `podcasts` in M2.A. The Compose UI reads
     * [subscriptions] which prefers the new field when present.
     */
    val library: List<PodcastSummary> = emptyList(),
    /**
     * Forward-compat alias for the M2.A `PodcastUpdate.podcasts` projection.
     * Empty until M2.A's FFI wiring lands; UI code should read [subscriptions]
     * which transparently falls back to [library].
     */
    @SerialName("podcasts") val podcasts: List<PodcastSummary> = emptyList(),
    /**
     * iTunes/RSS directory search results, populated by dispatching the
     * `{"op":"search_itunes","query":…}` action on the `podcast` namespace.
     * Mirror of `PodcastUpdate.search_results` (a `Vec<PodcastSummary>`).
     * Wire key is snake_case, so the explicit `@SerialName` is load-bearing —
     * kotlinx does not auto-convert.
     */
    @SerialName("search_results") val searchResults: List<PodcastSummary> = emptyList(),
    /**
     * Playback / app settings projection. Mirror of `PodcastUpdate.settings`.
     * The Rust side `skip_serializing_if = "is_default"`, so this key is
     * **absent** from the wire whenever settings equal the fresh-install
     * default — hence nullable here. Read with a `?: default` fallback.
     */
    val settings: SettingsSnapshot? = null,
    /** Playback "Up Next" queue, front-first. Mirror of `PodcastUpdate.queue`. */
    val queue: List<EpisodeSummary> = emptyList(),
    /** AI-triaged inbox, highest-priority first. Mirror of `PodcastUpdate.inbox`. */
    val inbox: List<InboxItem> = emptyList(),
    /**
     * `true` while the background LLM triage pass is running.
     * Mirror of `PodcastUpdate.inbox_triage_in_progress`. Drives the shimmer
     * indicator in the Inbox screen.
     */
    @SerialName("inbox_triage_in_progress") val inboxTriageInProgress: Boolean = false,
    /** Agent-scheduled task rows. Mirror of `PodcastUpdate.agent_tasks`. */
    @SerialName("agent_tasks") val agentTasks: List<AgentTaskSummary> = emptyList(),
    /**
     * Raw feedback events cached by the Rust feedback runtime. Android renders
     * [feedbackThreads]; this remains decoded for parity/debug surfaces only.
     */
    @SerialName("feedback_events") val feedbackEvents: List<JsonElement> = emptyList(),
    /** Resolved feedback threads emitted by `nmp-feedback`. */
    @SerialName("feedback_threads") val feedbackThreads: List<FeedbackThreadDto> = emptyList(),
) {
    /**
     * Effective subscription list — prefer the new `podcasts` projection, fall
     * back to the M2.F `library` field if the kernel hasn't migrated yet.
     */
    val subscriptions: List<PodcastSummary>
        get() = if (podcasts.isNotEmpty()) podcasts else library
}

/** One reply row in a resolved feedback thread. */
@Serializable
data class FeedbackReplyDto(
    @SerialName("event_id") val eventId: String = "",
    @SerialName("author_pubkey") val authorPubkey: String = "",
    val content: String = "",
    @SerialName("created_at") val createdAt: Long = 0,
)

/** Resolved feedback thread projected by `nmp-feedback`. */
@Serializable
data class FeedbackThreadDto(
    @SerialName("event_id") val eventId: String = "",
    @SerialName("author_pubkey") val authorPubkey: String = "",
    val category: String = "bug",
    val content: String = "",
    @SerialName("created_at") val createdAt: Long = 0,
    val title: String? = null,
    val summary: String? = null,
    @SerialName("status_label") val statusLabel: String? = null,
    val replies: List<FeedbackReplyDto> = emptyList(),
)

/**
 * Mirror of `apps/nmp-app-podcast/src/player/state.rs::PlayerState` (M13.C+D name).
 *
 * Used by `HomeScreen.NowPlayingCard` and `PlayerScreen`. Fields use snake_case
 * on the wire because the iOS `Codable` decoder — and the Rust struct itself —
 * speaks snake_case JSON.
 */
@Serializable
data class NowPlayingState(
    @SerialName("episode_id") val episodeId: String? = null,
    @SerialName("podcast_id") val podcastId: String? = null,
    @SerialName("episode_title") val episodeTitle: String? = null,
    @SerialName("podcast_title") val podcastTitle: String? = null,
    @SerialName("artwork_url") val artworkUrl: String? = null,
    @SerialName("position_secs") val positionSecs: Double = 0.0,
    @SerialName("duration_secs") val durationSecs: Double = 0.0,
    @SerialName("is_playing") val isPlaying: Boolean = false,
    val speed: Float = 1.0f,
    val volume: Float = 1.0f,
    @SerialName("sleep_timer_remaining_secs") val sleepTimerRemainingSecs: Long? = null,
    @SerialName("buffering_fraction") val bufferingFraction: Float? = null,
    @SerialName("last_error") val lastError: String? = null,
)

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
 * Mirror of `ConversationsSnapshot` (named `AgentSnapshot` here to match
 * the field name `agent: AgentSnapshot?` used in `PodcastSnapshot`).
 */
@Serializable
data class AgentSnapshot(
    @SerialName("active_count") val activeCount: Int = 0,
    @SerialName("pending_approvals") val pendingApprovals: List<PendingApprovalSnapshot> = emptyList(),
    @SerialName("latest_conversation_id") val latestConversationId: String? = null,
)

/**
 * Mirror of `PendingApprovalSnapshot`. `requestedAt` is Unix seconds.
 */
@Serializable
data class PendingApprovalSnapshot(
    val id: String,
    val description: String,
    @SerialName("requested_at") val requestedAt: Long,
)

/**
 * Mirror of `VoiceState` (named `VoiceStateSnapshot` on the Kotlin side).
 */
@Serializable
data class VoiceStateSnapshot(
    @SerialName("is_speaking") val isSpeaking: Boolean = false,
    @SerialName("current_request_id") val currentRequestId: String? = null,
    @SerialName("current_voice_id") val currentVoiceId: String? = null,
)

/**
 * Mirror of `BriefingSnapshot`. `status` is one of `"pending"`,
 * `"generating"`, `"ready"`, `"delivered"`, `"failed"`.
 */
@Serializable
data class BriefingSnapshot(
    val status: String = "pending",
    @SerialName("segment_count") val segmentCount: Int = 0,
    @SerialName("next_scheduled_minutes") val nextScheduledMinutes: Int? = null,
    /** Optional human-readable schedule label ("Daily at 7:00 AM"). */
    @SerialName("schedule_label") val scheduleLabel: String? = null,
)

/**
 * Mirror of `WidgetSnapshot`. Per D7 the kernel decides what to surface;
 * the widget renders the empty state when fields are `null`.
 */
@Serializable
data class WidgetSnapshot(
    @SerialName("now_playing_episode_title") val nowPlayingEpisodeTitle: String? = null,
    @SerialName("now_playing_podcast_title") val nowPlayingPodcastTitle: String? = null,
    @SerialName("now_playing_artwork_url") val nowPlayingArtworkUrl: String? = null,
    @SerialName("is_playing") val isPlaying: Boolean = false,
    @SerialName("position_fraction") val positionFraction: Float = 0.0f,
    @SerialName("unplayed_count") val unplayedCount: Int = 0,
)

/**
 * Identity projection mirrored from `PodcastUpdate.active_account`.
 * The Rust backend omits `activeAccount` entirely unless it can include both
 * the display npub and canonical hex account id.
 */
@Serializable
data class AccountSummary(
    val npub: String,
    @SerialName("pubkey_hex") val pubkeyHex: String,
    @SerialName("display_name") val displayName: String? = null,
    val mode: String = "local_key",
    @SerialName("picture_url") val pictureUrl: String? = null,
)

/**
 * One row of the library projection. Kept as the stable contract the
 * Compose UI compiles against even though the Rust serializer is still
 * on the M2.F stub.
 */
@Serializable
data class PodcastSummary(
    val id: String,
    val title: String,
    @SerialName("episode_count") val episodeCount: Int = 0,
    @SerialName("unplayed_count") val unplayedCount: Int = 0,
    @SerialName("artwork_url") val artworkUrl: String? = null,
    /**
     * RSS feed URL. Present for library rows and iTunes search results;
     * the key the search screen subscribes against (`{"op":"subscribe",
     * "feed_url":…}`). Mirror of `PodcastSummary.feed_url`.
     */
    @SerialName("feed_url") val feedUrl: String? = null,
    /** Podcast author / host. Mirror of `PodcastSummary.author`. */
    val author: String? = null,
    /** HTML-stripped podcast description. Mirror of `PodcastSummary.description`. */
    val description: String? = null,
    val episodes: List<EpisodeSummary> = emptyList(),
)

/**
 * Narrow episode row consumed by the Home "Recent Episodes" carousel and the
 * Show-detail list.
 */
@Serializable
data class EpisodeSummary(
    val id: String,
    val title: String,
    @SerialName("podcast_id") val podcastId: String? = null,
    @SerialName("podcast_title") val podcastTitle: String? = null,
    @SerialName("duration_secs") val durationSecs: Double? = null,
    @SerialName("artwork_url") val artworkUrl: String? = null,
    @SerialName("published_at") val publishedAt: Long? = null,
    /** Original RSS enclosure URL for streaming. Mirror of `EpisodeSummary.enclosure_url`. */
    @SerialName("enclosure_url") val enclosureUrl: String? = null,
    /** Episode show-notes / description (HTML — strip before rendering). */
    val description: String? = null,
    /** True once the user has listened to completion. */
    val played: Boolean = false,
    /** True when the episode is starred / bookmarked. */
    val starred: Boolean = false,
    /** On-disk path to the downloaded enclosure, or `null` when not downloaded. */
    @SerialName("download_path") val downloadPath: String? = null,
    /** Persisted resume position in seconds, or `null` for a fresh episode. */
    @SerialName("playback_position_secs") val playbackPositionSecs: Double? = null,
    /** Chapter markers. Mirror of `EpisodeSummary.chapters` (`Vec<ChapterSummary>`). */
    val chapters: List<ChapterSummary> = emptyList(),
    /** AI-assigned topic labels. Mirror of `EpisodeSummary.ai_categories`. */
    @SerialName("ai_categories") val aiCategories: List<String> = emptyList(),
    /** AI Inbox triage decision (`"inbox"` | `"archived"`), or `null` if untriaged. */
    @SerialName("triage_decision") val triageDecision: String? = null,
    /**
     * Publisher-provided transcript URL from the Podcasting 2.0
     * `<podcast:transcript>` tag. Non-null signals a transcript is available
     * to fetch via `podcast` `{"op":"fetch_transcript","episode_id":"..."}`.
     * Mirror of `EpisodeSummary.transcript_url`.
     */
    @SerialName("transcript_url") val transcriptUrl: String? = null,
    /**
     * Raw plain-text transcript, populated after a `fetch_transcript` action.
     * Mirror of `EpisodeSummary.transcript`.
     */
    val transcript: String? = null,
    /**
     * Structured transcript entries with timestamps.
     * Mirror of `EpisodeSummary.transcript_entries` (`Vec<TranscriptEntry>`).
     */
    @SerialName("transcript_entries") val transcriptEntries: List<TranscriptEntry> = emptyList(),
    /**
     * Transient transcript-ingestion status: `""` | `"queued"` | `"fetching_publisher"` |
     * `"transcribing"` | `"failed"`. Empty string means idle.
     * Mirror of `EpisodeSummary.transcript_status`.
     */
    @SerialName("transcript_status") val transcriptStatus: String = "",
    /**
     * User-facing error detail when `transcript_status == "failed"`.
     * Mirror of `EpisodeSummary.transcript_status_message`.
     */
    @SerialName("transcript_status_message") val transcriptStatusMessage: String? = null,
)

/**
 * One chapter marker on an episode. Mirror of the Rust
 * `ffi/projections/library.rs::ChapterSummary`.
 *
 * The wire field is `start_secs` (`f64`, **required**) — not the task-spec's
 * `start_time_secs`. `title` is a required `String` on the Rust side; defaulted
 * to empty here so a malformed row can't fail the whole snapshot decode.
 */
@Serializable
data class ChapterSummary(
    @SerialName("start_secs") val startSecs: Double = 0.0,
    @SerialName("end_secs") val endSecs: Double? = null,
    val title: String = "",
    @SerialName("image_url") val imageUrl: String? = null,
    val url: String? = null,
)

/**
 * One time-stamped transcript segment. Mirror of the Rust
 * `ffi/projections/library.rs::TranscriptEntry`.
 *
 * `speaker` is optional (not all ingestors emit per-segment speaker labels).
 * `endSecs` is optional (some ingestors don't provide segment end times).
 */
@Serializable
data class TranscriptEntry(
    @SerialName("start_secs") val startSecs: Double = 0.0,
    @SerialName("end_secs") val endSecs: Double? = null,
    val speaker: String? = null,
    val text: String = "",
)

/**
 * Android mirror of the settings fields currently consumed by playback and
 * provider/model configuration surfaces.
 */
@Serializable
data class SettingsSnapshot(
    @SerialName("default_playback_rate") val defaultPlaybackRate: Float = 1.0f,
    @SerialName("auto_delete_downloads_after_played") val autoDeleteDownloads: Boolean = false,
    @SerialName("agent_initial_model") val agentInitialModel: String = "deepseek-v4-flash:cloud",
    @SerialName("agent_initial_model_name") val agentInitialModelName: String = "DeepSeek Flash",
    @SerialName("agent_thinking_model") val agentThinkingModel: String = "deepseek-v4-pro:cloud",
    @SerialName("agent_thinking_model_name") val agentThinkingModelName: String = "DeepSeek Pro",
    @SerialName("memory_compilation_model") val memoryCompilationModel: String = "deepseek-v4-flash:cloud",
    @SerialName("memory_compilation_model_name") val memoryCompilationModelName: String = "DeepSeek Flash",
    @SerialName("wiki_model") val wikiModel: String = "deepseek-v4-flash:cloud",
    @SerialName("wiki_model_name") val wikiModelName: String = "DeepSeek Flash",
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
 * Mirror of `ffi/projections/agent.rs::AgentTaskSummary`. The Android shell
 * renders user-facing intent metadata and mutates rows through the
 * `podcast.tasks` action namespace. Internal dispatch namespace/body JSON is
 * intentionally not part of this public projection.
 */
@Serializable
data class AgentTaskSummary(
    val id: String = "",
    val title: String = "",
    val description: String? = null,
    @SerialName("intent_type") val intentType: String = "custom",
    @SerialName("intent_label") val intentLabel: String = "Custom task",
    @SerialName("intent_detail") val intentDetail: String? = null,
    val schedule: String = "",
    @SerialName("next_run_at") val nextRunAt: Long? = null,
    @SerialName("last_run_at") val lastRunAt: Long? = null,
    val status: String = "pending",
    @SerialName("is_enabled") val isEnabled: Boolean = false,
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

/** Lazy JSON parser shared by the snapshot consumer. */
object SnapshotCodec {
    private val json = Json {
        ignoreUnknownKeys = true
        coerceInputValues = true
    }

    /**
     * Decode a bare projection payload — the shape `KernelBridge.podcastSnapshot()`
     * returns straight off the projection cache (`{"running":...,"rev":...}`).
     */
    fun decode(raw: String?): PodcastSnapshot? =
        raw?.let { runCatching { json.decodeFromString<PodcastSnapshot>(it) }.getOrNull() }

    /**
     * Decode a reactive **push-frame** envelope — the shape
     * `KernelBridge.nextUpdate()` returns after the kernel's update callback
     * (`apps/nmp-app-podcast/src/android.rs::on_update`) decodes the binary
     * FlatBuffers frame to JSON: `{"t":"snapshot","v":{...}}`, where `v` is the
     * same `PodcastUpdate` projection [decode] consumes bare.
     *
     * Mirrors iOS `KernelBridge.swift::decodePodcastUpdate(envelopePayload:)`,
     * which unwraps `v` from the identical envelope. Non-`snapshot` tags (e.g.
     * `{"t":"panic",...}` from the D7 actor-death contract) and malformed frames
     * yield `null` so the caller simply keeps the last good snapshot.
     */
    fun decodeEnvelope(raw: String?): PodcastSnapshot? =
        raw?.let {
            runCatching {
                val envelope = json.decodeFromString<SnapshotEnvelope>(it)
                if (envelope.t == "snapshot" && envelope.v != null) {
                    json.decodeFromJsonElement(PodcastSnapshot.serializer(), envelope.v)
                } else {
                    null
                }
            }.getOrNull()
        }

    /**
     * Push-frame wrapper. `v` is the generic kernel snapshot value; we re-decode
     * it into the typed [PodcastSnapshot] only when `t == "snapshot"`.
     */
    @Serializable
    private data class SnapshotEnvelope(
        val t: String = "",
        val v: kotlinx.serialization.json.JsonElement? = null,
    )
}
