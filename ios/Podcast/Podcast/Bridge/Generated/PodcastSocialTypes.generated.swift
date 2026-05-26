// PodcastSocialTypes.generated.swift
// Social + discovery types: inbox, comments, contacts, categories, wiki, knowledge.
// Hand-maintained mirror of Rust projection types. See PodcastUpdate.generated.swift.

import Foundation

/// One row in the AI-triaged inbox surfaced via `PodcastUpdate.inbox`.
struct InboxItem: Codable, Identifiable, Equatable, Hashable {
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
    var aiCategories: [String] = []

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
struct SocialSnapshot: Codable, Equatable, Hashable {
    var following: [ContactSummary] = []
    var followingCount: Int = 0
}

/// One row in `PodcastUpdate.categories`. Backs the "Browse by Topic" grid.
struct CategoryBrowseItem: Codable, Identifiable, Equatable, Hashable {
    var category: String
    var episodeCount: Int = 0
    var podcastCount: Int = 0
    var topEpisodeIds: [String] = []
    var adSegments: [AdSegment]? = nil

    var id: String { category }
}

/// One AI-synthesised, per-podcast knowledge entry in `PodcastUpdate.wikiArticles`.
struct WikiArticle: Codable, Identifiable, Equatable, Hashable {
    var id: String
    var podcastId: String
    var topic: String
    var summary: String
    var sourceEpisodeIds: [String]? = nil
    var lastUpdatedAt: Int = 0
    var isGenerating: Bool = false
}

/// One row in the RAG / vector-search projection.
struct KnowledgeSearchResult: Codable, Identifiable, Equatable, Hashable {
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
