import Foundation

/// A single podcast known to the app — its identity and metadata.
///
/// Decoupled from `PodcastSubscription`. Knowing about a podcast does NOT
/// imply the user follows it: a `Podcast` row may have no corresponding
/// `PodcastSubscription`, which is how the agent's external-play and TTS-
/// generated flows attach episodes to a real show without forcing a follow.
///
/// Migration note: pre-split installs stored all of this inside
/// `PodcastSubscription`. The persistence layer splits each legacy row into a
/// `Podcast` (this type, keeping the legacy UUID as `id`) plus a slim
/// `PodcastSubscription` keyed by the same UUID. Episodes' `podcastID`
/// field carries the same UUID, so foreign keys keep working through the
/// rename.
struct Podcast: Codable, Sendable, Identifiable, Hashable {

    /// Source type. `.rss` is a feed-backed show; `.synthetic` covers the
    /// "Agent Generated" pseudo-podcast and the Unknown-fallback row.
    enum Kind: String, Codable, Sendable, Hashable {
        case rss
        case synthetic
    }

    /// Stable sentinel parent for episodes that arrived without a known
    /// podcast (e.g. `play_external_episode` invoked without a `feed_url`).
    /// Reuses the UUID of the dropped `Episode.externalSubscriptionID` so
    /// pre-split episodes that pointed at the old sentinel naturally point
    /// at this row after migration.
    static let unknownID = UUID(uuidString: "00000000-EEEE-EEEE-EEEE-000000000000")!

    var id: UUID
    var kind: Kind
    /// Original RSS / Atom feed URL. `nil` for `.synthetic` podcasts.
    var feedURL: URL?
    var title: String
    var author: String
    var imageURL: URL?
    var description: String
    /// BCP-47 language tag from `<language>` when present.
    var language: String?
    /// `<itunes:category>` text values, deduped, ordered as encountered.
    var categories: [String]
    /// When the app first learned about this podcast.
    var discoveredAt: Date
    /// `true` when this row's `title` is still the raw feed-host fallback
    /// inserted at placeholder-creation time and has NOT yet been overwritten
    /// by a successful metadata fetch. The UI may render these rows
    /// distinctly (faded title, retry chip, etc.).  Defaults to `false` so
    /// pre-existing persisted rows — which already have real titles — are
    /// unaffected after decode.
    var titleIsPlaceholder: Bool

    // MARK: - HTTP cache (feed polling)
    //
    // These move with the podcast — the feed URL lives here, so its HTTP
    // cache headers do too. Refresh only runs for podcasts the user follows
    // (a join with `PodcastSubscription` decides), but when it does, it
    // writes the new etag/lastModified back onto the podcast.

    /// Wall-clock of the last successful (200/304) feed fetch.
    var lastRefreshedAt: Date?
    /// `ETag` from the most recent feed response. Sent back as `If-None-Match`.
    var etag: String?
    /// `Last-Modified` from the most recent feed response. Sent back as
    /// `If-Modified-Since`. Stored as the raw HTTP date string to avoid
    /// timezone re-parsing churn.
    var lastModified: String?

    init(
        id: UUID = UUID(),
        kind: Kind = .rss,
        feedURL: URL? = nil,
        title: String,
        author: String = "",
        imageURL: URL? = nil,
        description: String = "",
        language: String? = nil,
        categories: [String] = [],
        discoveredAt: Date = Date(),
        lastRefreshedAt: Date? = nil,
        etag: String? = nil,
        lastModified: String? = nil,
        titleIsPlaceholder: Bool = false
    ) {
        self.id = id
        self.kind = kind
        self.feedURL = feedURL
        self.title = title
        self.author = author
        self.imageURL = imageURL
        self.description = description
        self.language = language
        self.categories = categories
        self.discoveredAt = discoveredAt
        self.lastRefreshedAt = lastRefreshedAt
        self.etag = etag
        self.lastModified = lastModified
        self.titleIsPlaceholder = titleIsPlaceholder
    }

    /// Built-in row inserted on store hydration when absent. The Unknown
    /// row backs episodes the agent added without a feed_url.
    static let unknown = Podcast(
        id: Podcast.unknownID,
        kind: .synthetic,
        feedURL: nil,
        title: "Unknown",
        author: "",
        imageURL: nil
    )

    private enum CodingKeys: String, CodingKey {
        case id, kind, feedURL, title, author, imageURL, description
        case language, categories, discoveredAt
        case lastRefreshedAt, etag, lastModified
        case titleIsPlaceholder
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(UUID.self, forKey: .id)
        kind = try c.decodeIfPresent(Kind.self, forKey: .kind) ?? .rss
        feedURL = try c.decodeIfPresent(URL.self, forKey: .feedURL)
        title = try c.decodeIfPresent(String.self, forKey: .title) ?? ""
        author = try c.decodeIfPresent(String.self, forKey: .author) ?? ""
        imageURL = try c.decodeIfPresent(URL.self, forKey: .imageURL)
        description = try c.decodeIfPresent(String.self, forKey: .description) ?? ""
        language = try c.decodeIfPresent(String.self, forKey: .language)
        categories = try c.decodeIfPresent([String].self, forKey: .categories) ?? []
        discoveredAt = try c.decodeIfPresent(Date.self, forKey: .discoveredAt) ?? Date()
        lastRefreshedAt = try c.decodeIfPresent(Date.self, forKey: .lastRefreshedAt)
        etag = try c.decodeIfPresent(String.self, forKey: .etag)
        lastModified = try c.decodeIfPresent(String.self, forKey: .lastModified)
        // `decodeIfPresent` + default `false` means pre-existing rows (written
        // before this field existed) decode cleanly without a migration shim.
        titleIsPlaceholder = try c.decodeIfPresent(Bool.self, forKey: .titleIsPlaceholder) ?? false
    }
}
