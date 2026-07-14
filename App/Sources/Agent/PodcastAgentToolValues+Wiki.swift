import Foundation

// MARK: - Wiki management tool value types

// MARK: - create_wiki_page result

/// Returned by `create_wiki_page` after the page is compiled and persisted.
public struct WikiCreateResult: Sendable, Equatable {
    public let pageID: String
    public let slug: String
    public let title: String
    public let kind: String
    public let summary: String
    public let claimCount: Int
    public let citationCount: Int
    public let confidence: Double

    public init(
        pageID: String,
        slug: String,
        title: String,
        kind: String,
        summary: String,
        claimCount: Int,
        citationCount: Int,
        confidence: Double
    ) {
        self.pageID = pageID
        self.slug = slug
        self.title = title
        self.kind = kind
        self.summary = summary
        self.claimCount = claimCount
        self.citationCount = citationCount
        self.confidence = confidence
    }
}

// MARK: - list_wiki_pages result

/// One row returned by `list_wiki_pages`.
public struct WikiPageListing: Sendable, Equatable {
    public let slug: String
    public let title: String
    public let kind: String
    public let summary: String
    public let confidence: Double
    public let generatedAt: Date
    public let citationCount: Int

    public init(
        slug: String,
        title: String,
        kind: String,
        summary: String,
        confidence: Double,
        generatedAt: Date,
        citationCount: Int
    ) {
        self.slug = slug
        self.title = title
        self.kind = kind
        self.summary = summary
        self.confidence = confidence
        self.generatedAt = generatedAt
        self.citationCount = citationCount
    }
}
