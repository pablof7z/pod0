package io.f7z.podcast

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement

/**
 * Per-domain push-frame envelope data classes.
 *
 * The kernel (NMP v0.5.0) emits typed sidecars via
 * `nmp_app_podcast_decode_update_frame`, which injects them into
 * `v.projections["podcast.<domain>"]` in the bridge JSON. The slim top-level
 * `v` carries only `rev`/`running`/`schema_version` and MUST NOT be decoded
 * as a full PodcastSnapshot — that caused the empty-library clobber bug.
 *
 * CONTRACT:
 *  - Each frame carries a `rev` field (monotonically increasing per-domain).
 *  - Absent domains in a given push frame are NOT present in `projections`;
 *    they MUST NOT overwrite prior state (delta suppression).
 *  - Tombstone shape: `{"rev":N,"<primary_field>":null}` — the primary field
 *    decodes as null, signalling the domain is now empty (sign-out, zero subs).
 *  - kotlinx-serialization `@SerialName` is used for snake_case fields;
 *    ignoreUnknownKeys = true for forward compat.
 *
 * Domain builders in `apps/nmp-app-podcast/src/ffi/snapshot_domain_projections.rs`:
 *   podcast.library   — rev, library, categories, search_results, nostr_results,
 *                       owned_podcasts, inbox, inbox_triage_in_progress,
 *                       inbox_last_triaged_at
 *   podcast.playback  — rev, now_playing, queue
 *   podcast.downloads — rev, downloads (null = no active downloads / tombstone)
 *   podcast.settings  — rev, settings, configured_relays
 *   podcast.identity  — rev, active_account (null = signed out / tombstone)
 *   podcast.widget    — rev, widget (null = nothing to show / tombstone)
 *   podcast.social    — rev, social (SocialSnapshot | null), nostr_conversations
 *   podcast.misc      — rev, agent_tasks, feedback_threads, feedback_events, voice,
 *                       agent, picks,
 *                       knowledge_search_results, memory_facts, clips, comments, agent_context
 *                       (social moved to podcast.social; flat agent_notes retired)
 */

// ── Schema IDs ────────────────────────────────────────────────────────────────

object DomainSchema {
    const val LIBRARY   = "podcast.library"
    const val PLAYBACK  = "podcast.playback"
    const val DOWNLOADS = "podcast.downloads"
    const val SETTINGS  = "podcast.settings"
    const val IDENTITY  = "podcast.identity"
    const val WIDGET    = "podcast.widget"
    const val SOCIAL    = "podcast.social"
    const val VOICE     = "podcast.voice"
    const val MISC      = "podcast.misc"
}

// ── podcast.library ───────────────────────────────────────────────────────────

@Serializable
data class LibraryDomainFrame(
    val rev: Long = 0,
    /** null = tombstone (all-unsubscribed or empty state). */
    val library: List<PodcastSummary>? = null,
    @SerialName("search_results") val searchResults: List<PodcastSummary>? = null,
    val inbox: List<InboxItem>? = null,
    @SerialName("inbox_triage_in_progress") val inboxTriageInProgress: Boolean? = null,
    @SerialName("inbox_last_triaged_at") val inboxLastTriagedAt: Long? = null,
)

// ── podcast.playback ──────────────────────────────────────────────────────────

@Serializable
data class PlaybackDomainFrame(
    val rev: Long = 0,
    @SerialName("now_playing") val nowPlaying: NowPlayingState? = null,
    val queue: List<EpisodeSummary>? = null,
)

// ── podcast.downloads ─────────────────────────────────────────────────────────

@Serializable
data class DownloadsDomainFrame(
    val rev: Long = 0,
    /** null = tombstone (all downloads cleared). */
    val downloads: DownloadQueueSnapshot? = null,
)

// ── podcast.settings ─────────────────────────────────────────────────────────

@Serializable
data class SettingsDomainFrame(
    val rev: Long = 0,
    val settings: SettingsSnapshot? = null,
)

// ── podcast.identity ─────────────────────────────────────────────────────────

@Serializable
data class IdentityDomainFrame(
    val rev: Long = 0,
    /** null = tombstone (signed out / no active account). */
    @SerialName("active_account") val activeAccount: AccountSummary? = null,
)

// ── podcast.widget ────────────────────────────────────────────────────────────

@Serializable
data class WidgetDomainFrame(
    val rev: Long = 0,
    /** null = tombstone (nothing to show). */
    val widget: WidgetSnapshot? = null,
)

// ── NMP resolved_profiles (top-level projections key) ────────────────────────

/**
 * One entry from the NMP kernel's `projections["resolved_profiles"]` map.
 *
 * Populated when the host claims a pubkey via `nmp_app_claim_profile`;
 * the kernel fetches kind:0 and surfaces the result here on the next push
 * frame. Mirrors the iOS `ResolvedProfile` struct in
 * `KernelIdentityProjection.swift`.
 *
 * `display` is the kernel's best display name (NIP-05 > display_name > name).
 * `pictureUrl` is the kind:0 picture field. Both are optional.
 *
 * Wire key is snake_case from Rust; `@SerialName` is load-bearing here
 * because kotlinx-serialization does NOT auto-convert snake_case → camelCase
 * without explicit configuration (the bridge JSON decoder uses
 * `convertFromSnakeCase` on iOS but Android uses `ignoreUnknownKeys` without
 * that strategy — so explicit `@SerialName` is required for every snake_case
 * field in Android DTO classes).
 */
@Serializable
data class ResolvedProfile(
    @SerialName("display_name") val display: String? = null,
    @SerialName("picture_url") val pictureUrl: String? = null,
)

// ── podcast.social ────────────────────────────────────────────────────────────

/**
 * One contact in the NIP-02 (kind:3) follow list.
 *
 * Mirror of `apps/nmp-app-podcast/src/ffi/projections/social.rs::ContactSummary`.
 * `npub` is pre-encoded bech32 for direct rendering. `pubkeyHex` is the raw
 * lowercase-hex pubkey used for `bridge.claimProfile(pubkeyHex)` to trigger
 * kind:0 profile resolution via the `resolved_profiles` seam (slice 2).
 *
 * Wire contract: `@SerialName` is load-bearing — Android does NOT
 * auto-convert snake_case → camelCase (no `convertFromSnakeCase` strategy).
 */
@Serializable
data class ContactSummaryDto(
    val npub: String = "",
    /** Raw lowercase-hex pubkey — used to call bridge.claimProfile for kind:0 resolution. */
    @SerialName("pubkey_hex") val pubkeyHex: String = "",
    @SerialName("display_name") val displayName: String? = null,
    @SerialName("picture_url") val pictureUrl: String? = null,
)

/**
 * NIP-02 follow-list snapshot projected by the social domain.
 *
 * Mirror of `apps/nmp-app-podcast/src/ffi/projections/social.rs::SocialSnapshot`.
 * `following` is the full list of contacts; `followingCount` equals
 * `following.size` and is provided as a sugar for badge rendering.
 */
@Serializable
data class SocialSnapshotDto(
    val following: List<ContactSummaryDto> = emptyList(),
    @SerialName("following_count") val followingCount: Int = 0,
)

/**
 * NIP-10-threaded Nostr conversation projection + NIP-02 follow graph.
 *
 * `social = null` arriving in this frame signals a tombstone (account switch
 * cleared all social state). `nostrConversations` follows the same pattern:
 * null = tombstone / cleared, absent = no change.
 *
 * Both `social` and `nostrConversations` are emitted atomically by the same
 * `build_social_payload` builder — a tombstone (`social = null`) clears
 * BOTH the follow list and the conversations list.
 *
 * NOTE: social moved OUT of podcast.misc into this domain in the
 * nostr-conversations-real-projection PR. The flat `agent_notes` field was
 * retired in chore/retire-flat-agent-notes-projection — conversations subsume it.
 */
@Serializable
data class SocialDomainFrame(
    val rev: Long = 0,
    /** NIP-02 follow-list snapshot. `null` = tombstone (account switch). */
    val social: SocialSnapshotDto? = null,
    /** User-curated friends. Empty list is authoritative and clears prior rows. */
    val friends: List<FriendSummary> = emptyList(),
    /** NIP-10-threaded conversations, newest-first by last_activity. */
    @SerialName("nostr_conversations") val nostrConversations: List<NostrConversationDto>? = null,
)

/**
 * Wire DTO for a single NIP-10-threaded Nostr conversation.
 * Kotlin consumers use this to render conversation lists and transcripts.
 */
@Serializable
data class NostrConversationDto(
    @SerialName("root_event_id")    val rootEventId: String = "",
    @SerialName("counterparty_hex") val counterpartyHex: String = "",
    val participants: List<String> = emptyList(),
    val turns: List<NostrConversationTurnDto> = emptyList(),
    /** Composed trust verdict: `(followed || approved) && !blocked`. */
    val trusted: Boolean = false,
    /**
     * Explicit block on the counterparty in the kernel `ApprovedPeerStore`.
     * Distinct from `trusted` so the trust menu can offer Unblock as the
     * recovery action. Mirror of `NostrConversationDTO.peer_blocked`.
     */
    @SerialName("peer_blocked")  val peerBlocked: Boolean = false,
    /**
     * Explicit approval on the counterparty (NOT follow-derived). A pure-follow
     * trusted peer reports `false`, so the menu avoids offering a no-op
     * "Remove approval". Mirror of `NostrConversationDTO.peer_approved`.
     */
    @SerialName("peer_approved") val peerApproved: Boolean = false,
    @SerialName("first_seen")   val firstSeen: Long = 0L,
    @SerialName("last_activity") val lastActivity: Long = 0L,
)

/** Wire DTO for a single turn in a Nostr conversation. */
@Serializable
data class NostrConversationTurnDto(
    @SerialName("event_id")    val eventId: String = "",
    val direction: String = "inbound",
    @SerialName("pubkey_hex")  val pubkeyHex: String = "",
    @SerialName("created_at") val createdAt: Long = 0L,
    val content: String = "",
)

// ── podcast.voice ─────────────────────────────────────────────────────────────

/**
 * Voice domain sidecar. Moved out of podcast.misc in PR #613.
 * `voice = null` is a tombstone (voice idle / conversation ended).
 * Mirror of `VoiceDomainFrame` in `KernelDomainFrames.swift`.
 */
@Serializable
data class VoiceDomainFrame(
    val rev: Long = 0,
    /** null = tombstone (voice idle / conversation ended). */
    val voice: VoiceStateSnapshot? = null,
)

// ── podcast.misc ──────────────────────────────────────────────────────────────

@Serializable
data class MiscDomainFrame(
    val rev: Long = 0,
    @SerialName("agent_tasks") val agentTasks: List<AgentTaskSummary>? = null,
    @SerialName("feedback_events") val feedbackEvents: List<JsonElement>? = null,
    @SerialName("feedback_threads") val feedbackThreads: List<FeedbackThreadDto>? = null,
    val agent: AgentSnapshot? = null,
    /** AI-curated picks rail. Populated by `picks_handler.rs`; null = not yet projected. */
    val picks: List<AgentPickSummary>? = null,
    /**
     * User-saved audio clips from `clip_handler::project_clips`. Null when the
     * kernel hasn't projected clips yet (no clips created this session). An
     * empty list is a valid state (all clips deleted). Null means "no change"
     * in the delta-merge protocol; empty list means "clips are empty now".
     *
     * Wire contract verified against `apps/nmp-app-podcast/src/ffi/projections/clips.rs`.
     * Snake_case keys: `episode_id`, `episode_title`, `podcast_title`, `start_secs`,
     * `end_secs`, `created_at` — all load-bearing `@SerialName` in [ClipSummary].
     */
    val clips: List<ClipSummary>? = null,
)

// ── Composite push-frame result ───────────────────────────────────────────────

/**
 * All per-domain frames extracted from one push frame. Only domains present in
 * the kernel's delta emit carry a non-null value. Absent domains MUST NOT
 * overwrite the last-accepted composite state.
 *
 * [resolvedProfiles] is extracted from `projections["resolved_profiles"]` in
 * the same top-level NMP projections map where `podcast.*` sidecars live. It
 * is NOT a `podcast.*` domain frame — it has no `rev` counter and is merged
 * additively (never cleared) into [PodcastSnapshot.resolvedProfiles].
 * An empty map means the kernel emitted no resolved profiles this tick.
 */
data class PodcastDomainFrames(
    val library:   LibraryDomainFrame? = null,
    val playback:  PlaybackDomainFrame? = null,
    val downloads: DownloadsDomainFrame? = null,
    val settings:  SettingsDomainFrame? = null,
    val identity:  IdentityDomainFrame? = null,
    val widget:    WidgetDomainFrame? = null,
    val social:    SocialDomainFrame? = null,
    val voice:     VoiceDomainFrame? = null,
    val misc:      MiscDomainFrame? = null,
    /** Additive resolved-profiles map from `projections["resolved_profiles"]`. */
    val resolvedProfiles: Map<String, ResolvedProfile> = emptyMap(),
) {
    val hasAnyDomain: Boolean get() =
        library != null || playback != null || downloads != null ||
        settings != null || identity != null || widget != null ||
        social != null || voice != null || misc != null || resolvedProfiles.isNotEmpty()

    fun presentDomainNames(): String {
        val names = mutableListOf<String>()
        if (library   != null) names.add("library")
        if (playback  != null) names.add("playback")
        if (downloads != null) names.add("downloads")
        if (settings  != null) names.add("settings")
        if (identity  != null) names.add("identity")
        if (widget    != null) names.add("widget")
        if (social    != null) names.add("social")
        if (voice     != null) names.add("voice")
        if (misc      != null) names.add("misc")
        if (resolvedProfiles.isNotEmpty()) names.add("resolved_profiles(${resolvedProfiles.size})")
        return if (names.isEmpty()) "none" else names.joinToString(",")
    }
}

// ── Per-domain rev tracker (drop-guard) ──────────────────────────────────────

/**
 * Holds the last-applied rev for each domain. A frame whose rev ≤ lastApplied
 * is stale and MUST be dropped without touching the composite snapshot.
 */
data class DomainRevTracker(
    var library:   Long = 0L,
    var playback:  Long = 0L,
    var downloads: Long = 0L,
    var settings:  Long = 0L,
    var identity:  Long = 0L,
    var widget:    Long = 0L,
    var social:    Long = 0L,
    var voice:     Long = 0L,
    var misc:      Long = 0L,
)
