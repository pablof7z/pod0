import Foundation
import os.log

// MARK: - LiveWikiStorageAdapter

/// Wraps `WikiStorage.shared` so the agent's wiki tools can query, create,
/// list, and delete wiki pages. Converted to a class so it can hold a weak
/// reference to `AppStateStore` for the model setting and API-key check
/// required by `createWikiPage`.
final class LiveWikiStorageAdapter: WikiStorageProtocol, @unchecked Sendable {

    let storage: WikiStorage
    weak var store: AppStateStore?

    nonisolated private static let logger = Logger.app("WikiStorageAdapter")

    init(storage: WikiStorage = .shared, store: AppStateStore? = nil) {
        self.storage = storage
        self.store = store
    }

    // MARK: - query_wiki

    func queryWiki(topic: String, scope: PodcastID?, limit: Int) async throws -> [WikiHit] {
        let trimmed = topic.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard !trimmed.isEmpty else { return [] }
        let scopeFilter: WikiScope? = scope.flatMap { UUID(uuidString: $0) }.map { .podcast($0) }
        let storage = storage
        let queryTokens = Self.tokenize(trimmed)
        let boundedLimit = max(1, limit)
        return try await Task.detached(priority: .utility) {
            let pages = try storage.allPages()
            let scored = pages.compactMap { page -> (page: WikiPage, score: Double, excerpt: String)? in
                if let scopeFilter, page.scope != scopeFilter { return nil }
                let score = Self.score(page: page, query: trimmed, tokens: queryTokens)
                guard score > 0 else { return nil }
                return (page, score, Self.excerpt(from: page, query: trimmed, tokens: queryTokens))
            }
            let filtered = scored
                .sorted {
                    if $0.score != $1.score { return $0.score > $1.score }
                    return $0.page.generatedAt > $1.page.generatedAt
                }
                .prefix(boundedLimit)
            return filtered.map { hit in
                WikiHit(
                    pageID: hit.page.id.uuidString,
                    title: hit.page.title,
                    excerpt: String(hit.excerpt.prefix(280)),
                    score: hit.score
                )
            }
        }.value
    }

    // MARK: - create_wiki_page

    func createWikiPage(title: String, kind: String, scope: PodcastID?) async throws -> WikiCreateResult {
        let trimmed = title.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            throw WikiAdapterError.invalidTitle
        }

        // Read model setting and check API key on MainActor (AppStateStore is @MainActor-isolated).
        // Fall back to Settings() default initializer so the canonical default lives in one place
        // and a nil store never silently overrides a user-configured model with a hardcoded literal.
        let model: String = await MainActor.run { [weak self] in
            (self?.store?.state.settings ?? Settings()).wikiModel
        }
        let reference = LLMModelReference(storedID: model)
        guard LLMProviderCredentialResolver.hasAPIKey(for: reference.provider) else {
            throw WikiAdapterError.noAPIKey(reference.provider.displayName)
        }

        // WikiGenerator accesses RAGService.shared.wikiRAG which is @MainActor-isolated.
        let storage = self.storage
        let generator: WikiGenerator = await MainActor.run {
            WikiGenerator(
                rag: RAGService.shared.wikiRAG,
                client: .live(model: model),
                storage: storage,
                model: model
            )
        }

        let wikiScope: WikiScope = scope.flatMap { UUID(uuidString: $0) }.map { .podcast($0) } ?? .global

        let result: WikiVerifyResult
        switch kind.lowercased() {
        case "person":
            result = try await generator.compilePerson(name: trimmed, scope: wikiScope)
        case "show":
            result = try await generator.compileShow(showName: trimmed, scope: wikiScope)
        default:
            result = try await generator.compileTopic(topic: trimmed, scope: wikiScope)
        }

        try generator.persist(result.page)
        Self.logger.info("createWikiPage: compiled '\(result.page.slug, privacy: .public)' model=\(model, privacy: .public) claims=\(result.keptClaims)")

        return WikiCreateResult(
            pageID: result.page.id.uuidString,
            slug: result.page.slug,
            title: result.page.title,
            kind: result.page.kind.rawValue,
            summary: result.page.summary,
            claimCount: result.page.allClaims.count,
            citationCount: result.page.allClaims.flatMap(\.citations).count,
            confidence: result.page.confidence
        )
    }

    // MARK: - list_wiki_pages

    func listWikiPages(scope: PodcastID?, limit: Int) async throws -> [WikiPageListing] {
        let wikiScope: WikiScope? = scope.flatMap { UUID(uuidString: $0) }.map { .podcast($0) }
        let storage = storage
        let boundedLimit = max(1, limit)
        return try await Task.detached(priority: .utility) {
            let entries = try storage.list(scope: wikiScope)
            return entries.prefix(boundedLimit).map { entry in
                WikiPageListing(
                    slug: entry.slug,
                    title: entry.title,
                    kind: entry.kind.rawValue,
                    summary: entry.summary,
                    confidence: entry.confidence,
                    generatedAt: entry.generatedAt,
                    citationCount: entry.citationCount
                )
            }
        }.value
    }

    // MARK: - delete_wiki_page

    func deleteWikiPage(slug: String, scope: PodcastID?) async throws {
        let wikiScope: WikiScope = scope.flatMap { UUID(uuidString: $0) }.map { .podcast($0) } ?? .global
        let storage = storage
        let normalizedSlug = WikiPage.normalize(slug: slug)
        try await Task.detached(priority: .utility) {
            try storage.delete(slug: normalizedSlug, scope: wikiScope)
        }.value
        Self.logger.info("deleteWikiPage: deleted '\(normalizedSlug, privacy: .public)'")
    }

    // MARK: - Scoring / excerpt helpers (query_wiki)

    private static func score(page: WikiPage, query: String, tokens: Set<String>) -> Double {
        let title = page.title.lowercased()
        let summary = page.summary.lowercased()
        let claims = page.allClaims.map { $0.text.lowercased() }
        var score = 0.0
        if title == query { score += 12 }
        if title.contains(query) { score += 8 }
        if summary.contains(query) { score += 5 }
        score += Double(claims.filter { $0.contains(query) }.count) * 4
        let corpusTokens = tokenize(([page.title, page.summary] + page.allClaims.map(\.text)).joined(separator: " "))
        let overlap = tokens.intersection(corpusTokens).count
        score += Double(overlap)
        return score
    }

    private static func excerpt(from page: WikiPage, query: String, tokens: Set<String>) -> String {
        let candidates = [page.summary] + page.allClaims.map(\.text)
        if let exact = candidates.first(where: { $0.lowercased().contains(query) }) {
            return exact
        }
        let best = candidates.max { lhs, rhs in
            tokenScore(lhs, tokens: tokens) < tokenScore(rhs, tokens: tokens)
        }
        if let best, !best.isBlank { return best }
        return page.summary
    }

    private static func tokenScore(_ text: String, tokens: Set<String>) -> Int {
        tokens.intersection(tokenize(text)).count
    }

    private static func tokenize(_ text: String) -> Set<String> {
        Set(text
            .lowercased()
            .split { !$0.isLetter && !$0.isNumber }
            .map(String.init)
            .filter { $0.count >= 2 })
    }
}

// MARK: - Errors

enum WikiAdapterError: LocalizedError {
    case invalidTitle
    case noAPIKey(String)

    var errorDescription: String? {
        switch self {
        case .invalidTitle:
            return "Title must not be empty."
        case .noAPIKey(let provider):
            return "No API key for \(provider). Add a key under Settings → AI Keys."
        }
    }
}
