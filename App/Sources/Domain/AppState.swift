import Foundation

// MARK: - AppState

struct AppState: Codable, Sendable {
    /// All podcasts the app knows about. Includes podcasts the user follows
    /// AND podcasts where the only attachment is an agent-added or
    /// manually-added episode. `state.subscriptions` is the projection of
    /// "podcasts the user actively follows".
    var podcasts: [Podcast] = []
    /// User's follow state. One row per followed podcast (`podcastID` FK).
    /// Many `Podcast` rows may exist without a matching subscription — that's
    /// "known but not followed."
    var subscriptions: [PodcastSubscription] = []
    /// All known episodes across all podcasts, hydrated from SQLite at
    /// launch. Reads filter by `podcastID` rather than maintaining
    /// per-podcast arrays so `upsertEpisodes(_:)` works for any feed.
    var episodes: [Episode] = []
    var notes: [Note] = []
    var friends: [Friend] = []
    var agentMemories: [AgentMemory] = []
    /// LLM-consolidated paragraph summarizing the active `agentMemories`.
    /// Produced by `AgentMemoryCompiler` after agent turns. When non-nil,
    /// `AgentPrompt` injects this single paragraph in place of the raw
    /// memory bullets so the prompt stays compact as memories accumulate.
    var compiledMemory: CompiledAgentMemory?
    /// Categories produced by `PodcastCategorizationService`. The other
    /// agent owns generation; we store them here so settings + UI surfaces
    /// share one source of truth. Defaults to empty so an uncategorized
    /// install behaves exactly as before.
    var categories: [PodcastCategory] = []
    /// Per-category user preferences keyed by `PodcastCategory.id`.
    var categorySettings: [UUID: CategorySettings] = [:]
    var settings: Settings = Settings()
    var nostrAllowedPubkeys: Set<String> = []
    var nostrBlockedPubkeys: Set<String> = []
    var nostrPendingApprovals: [NostrPendingApproval] = []
    /// One record per Nostr conversation root (NIP-10) the agent has
    /// participated in. Surfaces in Settings > Agent > Conversations.
    var nostrConversations: [NostrConversationRecord] = []
    /// Cached kind:0 profile metadata keyed by hex pubkey. Hydrated lazily
    /// when new pubkeys land in pending approvals or conversations.
    var nostrProfileCache: [String: NostrProfileMetadata] = [:]
    /// Event ids the agent has already produced a reply for (or has
    /// deliberately decided to skip). Persisted so a relay re-delivery
    /// across app restarts can't trigger a duplicate reply.
    var nostrRespondedEventIDs: Set<String> = []
    /// Highest `created_at` we've observed on an inbound kind:1 routed to
    /// the agent. Persisted so a future REQ can carry `since:` and skip
    /// already-seen events; bumped before the model runs so a crash
    /// mid-reply still advances the cursor (dedup via
    /// `nostrRespondedEventIDs` covers the small overlap window).
    var nostrSinceCursor: Int?
    var agentActivity: [AgentActivityEntry] = []
    /// User-authored transcript excerpts. See `Clip` and the composer in
    /// `App/Sources/Features/EpisodeDetail/Clip/`.
    var clips: [Clip] = []
    /// Cross-episode threading topics inferred by `ThreadingInferenceService`.
    /// Empty until the user runs a recompute (or the seed-mock path fires in
    /// Debug). UX-09 surfaces are reserved for >=3-mention patterns.
    var threadingTopics: [ThreadingTopic] = []
    /// Per-topic mentions powering the timeline view. One row per transcript
    /// span. Carries its own `topicID` so adapters can build the mention list
    /// without scanning the topic array.
    var threadingMentions: [ThreadingMention] = []
    /// Recurring tasks the agent has scheduled via `schedule_task`. Fired by
    /// `AgentScheduledTaskRunner` on foreground / app-appear. Persisted so
    /// tasks survive restarts.
    var agentScheduledTasks: [AgentScheduledTask] = []
    /// Outbound `send_friend_message` events awaiting a reply. When the
    /// friend's kind:1 response arrives, `NostrAgentResponder` claims the
    /// matching entry and re-invokes the originating conversation.
    var pendingFriendMessages: [PendingFriendMessage] = []
    /// The episode the user was most recently listening to. Persisted so the
    /// mini-player can be restored after an app restart without requiring the
    /// user to navigate back to the episode.
    var lastPlayedEpisodeID: UUID?

    init() {}

    private enum CodingKeys: String, CodingKey {
        case podcasts, subscriptions, episodes
        case notes, friends, agentMemories, compiledMemory, settings
        case categories, categorySettings
        case nostrAllowedPubkeys, nostrBlockedPubkeys, nostrPendingApprovals
        case nostrConversations, nostrProfileCache
        case nostrRespondedEventIDs, nostrSinceCursor
        case agentActivity
        case clips
        case threadingTopics, threadingMentions
        case agentScheduledTasks
        case pendingFriendMessages
        case lastPlayedEpisodeID
    }

    // Forward-compat: every field decoded with `decodeIfPresent` so adding new
    // fields never breaks decode of older persisted state. Legacy `items` /
    // `itemOrder` keys (from the inherited todo template) are silently ignored.
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)

        // Subscription rows: try slim shape (new format) first. If the file
        // was written by a pre-split build, the rows carry legacy keys
        // (feedURL, title, …); split each into a Podcast + slim subscription
        // (subscribers only — synthetic / agent-generated rows become
        // Podcast-only, with no auto-follow).
        let (decodedPodcasts, decodedSubscriptions) = try Self.decodeSubscriptions(from: c)
        podcasts = try c.decodeIfPresent([Podcast].self, forKey: .podcasts) ?? decodedPodcasts
        subscriptions = decodedSubscriptions
        // Ensure the Unknown podcast row always exists so episodes that point
        // at `Podcast.unknownID` (including pre-split externalSubscriptionID
        // episodes that share the same UUID) resolve.
        if !podcasts.contains(where: { $0.id == Podcast.unknownID }) {
            podcasts.append(Podcast.unknown)
        }

        episodes = try c.decodeIfPresent([Episode].self, forKey: .episodes) ?? []
        notes = try c.decodeIfPresent([Note].self, forKey: .notes) ?? []
        friends = try c.decodeIfPresent([Friend].self, forKey: .friends) ?? []
        agentMemories = try c.decodeIfPresent([AgentMemory].self, forKey: .agentMemories) ?? []
        compiledMemory = try c.decodeIfPresent(CompiledAgentMemory.self, forKey: .compiledMemory)
        categories = try c.decodeIfPresent([PodcastCategory].self, forKey: .categories) ?? []
        categorySettings = try c.decodeIfPresent([UUID: CategorySettings].self, forKey: .categorySettings) ?? [:]
        settings = try c.decodeIfPresent(Settings.self, forKey: .settings) ?? Settings()
        nostrAllowedPubkeys = try c.decodeIfPresent(Set<String>.self, forKey: .nostrAllowedPubkeys) ?? []
        nostrBlockedPubkeys = try c.decodeIfPresent(Set<String>.self, forKey: .nostrBlockedPubkeys) ?? []
        nostrPendingApprovals = try c.decodeIfPresent([NostrPendingApproval].self, forKey: .nostrPendingApprovals) ?? []
        nostrConversations = try c.decodeIfPresent([NostrConversationRecord].self, forKey: .nostrConversations) ?? []
        nostrProfileCache = try c.decodeIfPresent([String: NostrProfileMetadata].self, forKey: .nostrProfileCache) ?? [:]
        nostrRespondedEventIDs = try c.decodeIfPresent(Set<String>.self, forKey: .nostrRespondedEventIDs) ?? []
        nostrSinceCursor = try c.decodeIfPresent(Int.self, forKey: .nostrSinceCursor)
        agentActivity = try c.decodeIfPresent([AgentActivityEntry].self, forKey: .agentActivity) ?? []
        clips = try c.decodeIfPresent([Clip].self, forKey: .clips) ?? []
        threadingTopics = try c.decodeIfPresent([ThreadingTopic].self, forKey: .threadingTopics) ?? []
        threadingMentions = try c.decodeIfPresent([ThreadingMention].self, forKey: .threadingMentions) ?? []
        agentScheduledTasks = try c.decodeIfPresent([AgentScheduledTask].self, forKey: .agentScheduledTasks) ?? []
        pendingFriendMessages = try c.decodeIfPresent([PendingFriendMessage].self, forKey: .pendingFriendMessages) ?? []
        lastPlayedEpisodeID = try c.decodeIfPresent(UUID.self, forKey: .lastPlayedEpisodeID)
    }

    /// Decodes the `subscriptions` array, handling both the new slim shape
    /// (rows carrying only `podcastID` + user prefs) and the pre-split
    /// shape (combined Podcast + PodcastSubscription rows). Returns the
    /// derived podcasts (empty when the new shape is in use — the new shape
    /// reads podcasts from the top-level `podcasts` key) and the slim
    /// subscription rows.
    private static func decodeSubscriptions(
        from c: KeyedDecodingContainer<CodingKeys>
    ) throws -> (podcasts: [Podcast], subscriptions: [PodcastSubscription]) {
        // If the persisted file has a top-level `podcasts` key, it's the new
        // shape — read subscriptions as-is.
        if c.contains(.podcasts) {
            let slim = try c.decodeIfPresent([PodcastSubscription].self, forKey: .subscriptions) ?? []
            return ([], slim)
        }
        // Pre-split file. Decode each row as a legacy shape and split.
        let legacy = try c.decodeIfPresent([LegacyPodcastSubscriptionRow].self, forKey: .subscriptions) ?? []
        var derivedPodcasts: [Podcast] = []
        var derivedSubscriptions: [PodcastSubscription] = []
        derivedPodcasts.reserveCapacity(legacy.count)
        derivedSubscriptions.reserveCapacity(legacy.count)
        for row in legacy {
            derivedPodcasts.append(row.toPodcast())
            // Synthetic (Agent Generated) rows were "auto-subscribed" only
            // because the old data model had no concept of a podcast without
            // a subscription. In the split model they become Podcast-only —
            // no notifications / no auto-download / no row in the user's
            // subscriptions list.
            if !(row.isAgentGenerated ?? false) {
                derivedSubscriptions.append(row.toSubscription())
            }
        }
        return (derivedPodcasts, derivedSubscriptions)
    }
}

// MARK: - Legacy subscription decode shape

/// Mirror of the pre-split `PodcastSubscription` on-disk shape. Used only
/// during decode of files written by older builds; never encoded.
private struct LegacyPodcastSubscriptionRow: Decodable {
    var id: UUID
    var feedURL: URL
    var title: String?
    var author: String?
    var imageURL: URL?
    var description: String?
    var language: String?
    var categories: [String]?
    var subscribedAt: Date?
    var lastRefreshedAt: Date?
    var etag: String?
    var lastModified: String?
    var autoDownload: AutoDownloadPolicy?
    var notificationsEnabled: Bool?
    var defaultPlaybackRate: Double?
    var isAgentGenerated: Bool?

    func toPodcast() -> Podcast {
        Podcast(
            id: id,
            kind: (isAgentGenerated ?? false) ? .synthetic : .rss,
            feedURL: feedURL,
            title: title ?? "",
            author: author ?? "",
            imageURL: imageURL,
            description: description ?? "",
            language: language,
            categories: categories ?? [],
            discoveredAt: subscribedAt ?? Date(),
            lastRefreshedAt: lastRefreshedAt,
            etag: etag,
            lastModified: lastModified
        )
    }

    func toSubscription() -> PodcastSubscription {
        PodcastSubscription(
            podcastID: id,
            subscribedAt: subscribedAt ?? Date(),
            autoDownload: autoDownload ?? .default,
            notificationsEnabled: notificationsEnabled ?? true,
            defaultPlaybackRate: defaultPlaybackRate
        )
    }
}
