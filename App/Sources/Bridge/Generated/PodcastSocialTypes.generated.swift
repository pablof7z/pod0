// PodcastSocialTypes.generated.swift
// Social + discovery types: inbox, comments, contacts, categories, wiki, knowledge.
// Hand-maintained mirror of Rust projection types. See PodcastUpdate.generated.swift.

import Foundation

/// One row in the AI-triaged inbox surfaced via `PodcastUpdate.inbox`.
struct InboxItem: Identifiable, Equatable, Hashable {
    var episodeId: String
    var episodeTitle: String
    var podcastId: String
    var podcastTitle: String
    var artworkUrl: String? = nil
    var publishedAt: Int
    var durationSecs: Double? = nil
    /// `0.0..=1.0`; higher = more important.
    var priorityScore: Double
    var priorityReason: String? = nil
    @DefaultEmptyStrings var aiCategories: [String] = []

    var id: String { episodeId }
}

/// One NIP-22 (kind 1111) comment row in `PodcastUpdate.comments`.
struct CommentSummary: Codable, Identifiable, Equatable, Hashable {
    var id: String
    var authorNpub: String
    var authorName: String? = nil
    var content: String
    var createdAt: Int
}

/// One contact in the active account's NIP-02 (kind:3) follow list.
struct ContactSummary: Codable, Identifiable, Equatable, Hashable {
    var npub: String
    var displayName: String? = nil
    var pictureUrl: String? = nil

    var id: String { npub }
}

/// Snapshot of the user's Nostr social graph (NIP-02 / kind:3 follows).
struct SocialSnapshot: Equatable, Hashable {
    var following: [ContactSummary] = []
    var followingCount: Int = 0
}

/// One row in `PodcastUpdate.categories`. Backs the "Browse by Topic" grid.
struct CategoryBrowseItem: Identifiable, Equatable, Hashable {
    var category: String
    var episodeCount: Int = 0
    var podcastCount: Int = 0
    var topEpisodeIds: [String] = []
    var adSegments: [AdSegment]? = nil

    var id: String { category }
}

/// One AI-synthesised, per-podcast knowledge entry in `PodcastUpdate.wikiArticles`.
struct WikiArticle: Identifiable, Equatable, Hashable {
    var id: String
    var podcastId: String
    var topic: String
    var summary: String
    var sourceEpisodeIds: [String]? = nil
    var lastUpdatedAt: Int = 0
    var isGenerating: Bool = false
}

/// One row in the RAG / vector-search projection.
struct KnowledgeSearchResult: Identifiable, Equatable, Hashable {
    var episodeId: String
    var episodeTitle: String
    var podcastTitle: String
    var snippet: String
    var startSecs: Double? = nil
    var relevanceScore: Double = 0

    var id: String { "\(episodeId)|\(snippet.hashValue)" }
}

/// One key→value fact the agent or user saved via the memory system.
struct MemoryFact: Codable, Identifiable, Equatable, Hashable {
    var id: String
    var key: String
    var value: String
    var source: String
    var createdAt: Int
}

// MARK: - Custom Decodable implementations
//
// Rust uses `#[serde(default, skip_serializing_if)]` on bool fields (omit when
// false) and Vec fields (omit when empty). Conformance is declared in extensions
// (not struct bodies) so the synthesized memberwise init is preserved.

extension InboxItem: Codable {
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        episodeId = try c.decode(String.self, forKey: .episodeId)
        episodeTitle = try c.decode(String.self, forKey: .episodeTitle)
        podcastId = try c.decode(String.self, forKey: .podcastId)
        podcastTitle = try c.decode(String.self, forKey: .podcastTitle)
        artworkUrl = try c.decodeIfPresent(String.self, forKey: .artworkUrl)
        publishedAt = try c.decode(Int.self, forKey: .publishedAt)
        durationSecs = try c.decodeIfPresent(Double.self, forKey: .durationSecs)
        priorityScore = try c.decode(Double.self, forKey: .priorityScore)
        priorityReason = try c.decodeIfPresent(String.self, forKey: .priorityReason)
        aiCategories = try c.decodeIfPresent([String].self, forKey: .aiCategories) ?? []
    }
}

extension SocialSnapshot: Codable {
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        following = try c.decodeIfPresent([ContactSummary].self, forKey: .following) ?? []
        followingCount = try c.decodeIfPresent(Int.self, forKey: .followingCount) ?? 0
    }
}

extension CategoryBrowseItem: Codable {
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        category = try c.decode(String.self, forKey: .category)
        episodeCount = try c.decodeIfPresent(Int.self, forKey: .episodeCount) ?? 0
        podcastCount = try c.decodeIfPresent(Int.self, forKey: .podcastCount) ?? 0
        topEpisodeIds = try c.decodeIfPresent([String].self, forKey: .topEpisodeIds) ?? []
        adSegments = try c.decodeIfPresent([AdSegment].self, forKey: .adSegments)
    }
}

extension WikiArticle: Codable {
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        podcastId = try c.decode(String.self, forKey: .podcastId)
        topic = try c.decode(String.self, forKey: .topic)
        summary = try c.decode(String.self, forKey: .summary)
        sourceEpisodeIds = try c.decodeIfPresent([String].self, forKey: .sourceEpisodeIds)
        lastUpdatedAt = try c.decodeIfPresent(Int.self, forKey: .lastUpdatedAt) ?? 0
        isGenerating = try c.decodeIfPresent(Bool.self, forKey: .isGenerating) ?? false
    }
}

extension KnowledgeSearchResult: Codable {
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        episodeId = try c.decode(String.self, forKey: .episodeId)
        episodeTitle = try c.decode(String.self, forKey: .episodeTitle)
        podcastTitle = try c.decode(String.self, forKey: .podcastTitle)
        snippet = try c.decode(String.self, forKey: .snippet)
        startSecs = try c.decodeIfPresent(Double.self, forKey: .startSecs)
        relevanceScore = try c.decodeIfPresent(Double.self, forKey: .relevanceScore) ?? 0
    }
}
