package io.f7z.podcast

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.DeserializationStrategy
import kotlinx.serialization.builtins.MapSerializer
import kotlinx.serialization.builtins.serializer
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

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
 *                       owned_podcasts, inbox, inbox_triage_in_progress
 *   podcast.playback  — rev, now_playing, queue
 *   podcast.downloads — rev, downloads (null = no active downloads / tombstone)
 *   podcast.settings  — rev, settings, configured_relays
 *   podcast.identity  — rev, active_account (null = signed out / tombstone)
 *   podcast.widget    — rev, widget (null = nothing to show / tombstone)
 *   podcast.social    — rev, social (SocialSnapshot | null), nostr_conversations
 *   podcast.misc      — rev, agent_tasks, feedback_threads, feedback_events, voice,
 *                       agent, wiki_articles, wiki_search_results, picks,
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
 * NIP-10-threaded Nostr conversation projection + NIP-02 follow graph.
 *
 * `social = null` arriving in this frame signals a tombstone (account switch
 * cleared all social state). `nostrConversations` follows the same pattern:
 * null = tombstone / cleared, absent = no change.
 *
 * NOTE: social moved OUT of podcast.misc into this domain in the
 * nostr-conversations-real-projection PR. The flat `agent_notes` field was
 * retired in chore/retire-flat-agent-notes-projection — conversations subsume it.
 */
@Serializable
data class SocialDomainFrame(
    val rev: Long = 0,
    /** NIP-02 follow-list snapshot. `null` = tombstone (account switch). */
    val social: JsonElement? = null,
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
    val trusted: Boolean = false,
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

// ── podcast.misc ──────────────────────────────────────────────────────────────

@Serializable
data class MiscDomainFrame(
    val rev: Long = 0,
    @SerialName("agent_tasks") val agentTasks: List<AgentTaskSummary>? = null,
    @SerialName("feedback_events") val feedbackEvents: List<JsonElement>? = null,
    @SerialName("feedback_threads") val feedbackThreads: List<FeedbackThreadDto>? = null,
    val voice: VoiceStateSnapshot? = null,
    val agent: AgentSnapshot? = null,
    /** AI-curated picks rail. Populated by `picks_handler.rs`; null = not yet projected. */
    val picks: List<AgentPickSummary>? = null,
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
    val misc:      MiscDomainFrame? = null,
    /** Additive resolved-profiles map from `projections["resolved_profiles"]`. */
    val resolvedProfiles: Map<String, ResolvedProfile> = emptyMap(),
) {
    val hasAnyDomain: Boolean get() =
        library != null || playback != null || downloads != null ||
        settings != null || identity != null || widget != null ||
        social != null || misc != null || resolvedProfiles.isNotEmpty()

    fun presentDomainNames(): String {
        val names = mutableListOf<String>()
        if (library   != null) names.add("library")
        if (playback  != null) names.add("playback")
        if (downloads != null) names.add("downloads")
        if (settings  != null) names.add("settings")
        if (identity  != null) names.add("identity")
        if (widget    != null) names.add("widget")
        if (social    != null) names.add("social")
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
    var misc:      Long = 0L,
)

// ── Updated SnapshotCodec ─────────────────────────────────────────────────────

/**
 * Lazy JSON parser shared by the snapshot consumer.
 *
 * NMP v0.5.0 push frames carry per-domain typed sidecars injected by
 * `nmp_app_podcast_decode_update_frame` under `v.projections[schema_id]`.
 * The slim `v` envelope itself carries only `rev`/`running`/`schema_version`
 * and MUST NOT be decoded directly as a PodcastSnapshot.
 *
 * `decodeDomainFrames` is the correct entry point for push frames.
 * `decode` remains for the initial cold-start pull from `podcastSnapshot()`.
 */
object SnapshotCodec {
    val json = Json {
        ignoreUnknownKeys = true
        coerceInputValues = true
    }

    /**
     * Decode a bare projection payload — the shape `KernelBridge.podcastSnapshot()`
     * returns straight off the projection cache. Used for the initial cold-start paint.
     */
    fun decode(raw: String?): PodcastSnapshot? =
        raw?.let { runCatching { json.decodeFromString<PodcastSnapshot>(it) }.getOrNull() }

    /**
     * Decode per-domain sidecars from a reactive push-frame envelope.
     *
     * The envelope shape is:
     * ```json
     * { "t": "snapshot", "v": { "rev": N, "running": true,
     *     "projections": {
     *         "podcast.playback": { "rev": N, "now_playing": {...}, "queue": [...] },
     *         "podcast.library":  { "rev": N, "library": [...], ... },
     *         ...
     *     }
     * }}
     * ```
     *
     * Only domains whose sidecar is present in `projections` are populated.
     * Absent domains → null field → caller MUST NOT overwrite prior state.
     * Tombstone: `{"rev":N,"<field>":null}` — primary field decodes null → clear slice.
     *
     * Returns null when the envelope is not a "snapshot" frame, is unparseable,
     * or carries no podcast.* projections — D6 degrade, never throws.
     */
    fun decodeDomainFrames(raw: String?): PodcastDomainFrames? {
        raw ?: return null
        return runCatching {
            val outerObj = json.parseToJsonElement(raw) as? JsonObject ?: return null
            val t = outerObj["t"]?.jsonPrimitive?.content ?: return null
            if (t != "snapshot") return null

            val v = outerObj["v"]?.jsonObject ?: return null
            val projections = v["projections"]?.jsonObject ?: return null

            fun <T> tryDecode(key: String, deserializer: DeserializationStrategy<T>): T? {
                val elem = projections[key] ?: return null
                return runCatching { json.decodeFromJsonElement(deserializer, elem) }.getOrNull()
            }

            val library   = tryDecode(DomainSchema.LIBRARY,   LibraryDomainFrame.serializer())
            val playback  = tryDecode(DomainSchema.PLAYBACK,  PlaybackDomainFrame.serializer())
            val downloads = tryDecode(DomainSchema.DOWNLOADS, DownloadsDomainFrame.serializer())
            val settings  = tryDecode(DomainSchema.SETTINGS,  SettingsDomainFrame.serializer())
            val identity  = tryDecode(DomainSchema.IDENTITY,  IdentityDomainFrame.serializer())
            val widget    = tryDecode(DomainSchema.WIDGET,     WidgetDomainFrame.serializer())
            val social    = tryDecode(DomainSchema.SOCIAL,     SocialDomainFrame.serializer())
            val misc      = tryDecode(DomainSchema.MISC,       MiscDomainFrame.serializer())

            // `resolved_profiles` lives in the NMP-level projections map (not a
            // `podcast.*` domain sidecar). Decode it additively — the map is an
            // object keyed by hex pubkey; absent = no claimed profiles resolved
            // this tick. D6: any decode error yields an empty map (not a failure).
            val resolvedProfiles: Map<String, ResolvedProfile> = run decodeProfiles@{
                val elem = projections["resolved_profiles"]
                    ?: return@decodeProfiles emptyMap()
                runCatching {
                    json.decodeFromJsonElement(
                        MapSerializer(String.serializer(), ResolvedProfile.serializer()),
                        elem,
                    )
                }.getOrDefault(emptyMap())
            }

            val frames = PodcastDomainFrames(
                library          = library,
                playback         = playback,
                downloads        = downloads,
                settings         = settings,
                identity         = identity,
                widget           = widget,
                social           = social,
                misc             = misc,
                resolvedProfiles = resolvedProfiles,
            )
            if (!frames.hasAnyDomain) null else frames
        }.getOrNull()
    }

    /**
     * Merge present domain frames into a held [PodcastSnapshot] via copy().
     *
     * Per-domain drop-guard: if frame.rev <= tracker[domain], the frame is
     * stale and that domain slice is skipped (no clobber). The tracker is
     * updated in-place for each accepted domain.
     *
     * Tombstone handling: a domain's primary field arriving as null clears
     * that slice (e.g. identity.activeAccount = null → signed out).
     *
     * Returns the updated snapshot (may be the same instance if all domains
     * were stale), plus a flag indicating whether any domain was accepted.
     */
    fun mergeFrames(
        frames: PodcastDomainFrames,
        current: PodcastSnapshot,
        tracker: DomainRevTracker,
    ): Pair<PodcastSnapshot, Boolean> {
        var snap = current
        var anyAccepted = false

        // ── library ──────────────────────────────────────────────────────────
        frames.library?.let { lib ->
            if (lib.rev > tracker.library) {
                tracker.library = lib.rev
                anyAccepted = true
                snap = snap.copy(
                    // null = tombstone → clear to empty list
                    library  = lib.library ?: emptyList(),
                    podcasts = lib.library ?: emptyList(),
                    searchResults      = lib.searchResults ?: emptyList(),
                    inbox              = lib.inbox ?: emptyList(),
                    inboxTriageInProgress = lib.inboxTriageInProgress ?: false,
                )
            }
        }

        // ── playback ─────────────────────────────────────────────────────────
        frames.playback?.let { play ->
            if (play.rev > tracker.playback) {
                tracker.playback = play.rev
                anyAccepted = true
                snap = snap.copy(
                    nowPlaying = play.nowPlaying,
                    queue      = play.queue ?: emptyList(),
                )
            }
        }

        // ── downloads ────────────────────────────────────────────────────────
        frames.downloads?.let { dl ->
            if (dl.rev > tracker.downloads) {
                tracker.downloads = dl.rev
                anyAccepted = true
                // null = tombstone → no active downloads
                snap = snap.copy(downloads = dl.downloads)
            }
        }

        // ── settings ─────────────────────────────────────────────────────────
        frames.settings?.let { sett ->
            if (sett.rev > tracker.settings) {
                tracker.settings = sett.rev
                anyAccepted = true
                if (sett.settings != null) {
                    snap = snap.copy(settings = sett.settings)
                }
            }
        }

        // ── identity ─────────────────────────────────────────────────────────
        frames.identity?.let { ident ->
            if (ident.rev > tracker.identity) {
                tracker.identity = ident.rev
                anyAccepted = true
                // null = tombstone → signed out
                snap = snap.copy(activeAccount = ident.activeAccount)
            }
        }

        // ── widget ───────────────────────────────────────────────────────────
        frames.widget?.let { wid ->
            if (wid.rev > tracker.widget) {
                tracker.widget = wid.rev
                anyAccepted = true
                // null = tombstone → nothing to show
                snap = snap.copy(widget = wid.widget)
            }
        }

        // ── social ───────────────────────────────────────────────────────────
        // Kernel-authoritative: social moved out of podcast.misc.
        // nostrConversations is wired into PodcastSnapshot so the conversations
        // list + detail screens can render directly from the snapshot flow.
        // null = tombstone (account switch / no conversations) → clear to empty.
        // The flat agent_notes field was retired; conversations subsume it.
        frames.social?.let { soc ->
            if (soc.rev > tracker.social) {
                tracker.social = soc.rev
                anyAccepted = true
                snap = snap.copy(
                    nostrConversations = soc.nostrConversations ?: emptyList(),
                )
            }
        }

        // ── misc ─────────────────────────────────────────────────────────────
        // NOTE: social moved to podcast.social (above).
        frames.misc?.let { m ->
            if (m.rev > tracker.misc) {
                tracker.misc = m.rev
                anyAccepted = true
                snap = snap.copy(
                    agentTasks      = m.agentTasks ?: snap.agentTasks,
                    feedbackEvents  = m.feedbackEvents ?: snap.feedbackEvents,
                    feedbackThreads = m.feedbackThreads ?: snap.feedbackThreads,
                    voice           = m.voice ?: snap.voice,
                    agent           = m.agent ?: snap.agent,
                    picks           = m.picks ?: snap.picks,
                )
            }
        }

        // ── resolved_profiles (additive, no rev gate) ─────────────────────────
        // The NMP kernel emits `projections["resolved_profiles"]` whenever a
        // claimed profile resolves (T114 reference-first profile resolution).
        // Unlike podcast domain sidecars, this map has no `rev` counter — it is
        // always an additive delta (entries only added, never removed mid-session).
        // Merging is a simple union: new entries from this frame win (the kernel
        // never downgrades a profile), existing entries without a new value are
        // retained from the composite. This mirrors iOS
        // `AppStateStore.mergeResolvedProfiles`.
        if (frames.resolvedProfiles.isNotEmpty()) {
            anyAccepted = true
            snap = snap.copy(
                resolvedProfiles = snap.resolvedProfiles + frames.resolvedProfiles,
            )
        }

        return Pair(snap, anyAccepted)
    }
}
