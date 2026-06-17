import Foundation

// MARK: - LivePodcastInventoryAdapter
//
// Concrete inventory/category implementation backed by `AppStateStore`.
// Most methods are pure reads off `state` plus a sort, so this stays
// allocation-light even on libraries with thousands of episodes.
//
// Constructed once per `AgentChatSession` (via `LivePodcastAgentToolDeps.make`)
// with a weak reference to the store so the adapter never extends the store's
// lifetime.

@MainActor
final class LivePodcastInventoryAdapter: PodcastInventoryProtocol, PodcastCategoryProtocol, @unchecked Sendable {

    private weak var store: AppStateStore?

    init(store: AppStateStore) {
        self.store = store
    }

    // MARK: - PodcastInventoryProtocol

    func listSubscriptions(limit: Int) async -> [SubscriptionSummary] {
        await MainActor.run { listSubscriptionsSync(limit: limit) }
    }

    func listPodcasts(limit: Int) async -> [PodcastInventoryRow] {
        await MainActor.run { listPodcastsSync(limit: limit) }
    }

    func listEpisodes(podcastID: PodcastID, limit: Int) async -> [EpisodeInventoryRow]? {
        await MainActor.run { listEpisodesSync(podcastID: podcastID, limit: limit) }
    }

    func listInProgress(limit: Int) async -> [EpisodeInventoryRow] {
        await MainActor.run { listInProgressSync(limit: limit) }
    }

    func listRecentUnplayed(limit: Int) async -> [EpisodeInventoryRow] {
        await MainActor.run { listRecentUnplayedSync(limit: limit) }
    }

    // MARK: - PodcastCategoryProtocol

    func listCategories(limit: Int, includePodcasts: Bool) async -> [PodcastCategorySummary] {
        await MainActor.run {
            listCategoriesSync(limit: limit, includePodcasts: includePodcasts)
        }
    }

    func changePodcastCategory(
        podcastID: PodcastID,
        category reference: PodcastCategoryReference
    ) async throws -> PodcastCategoryChangeResult {
        try await MainActor.run {
            try changePodcastCategorySync(podcastID: podcastID, category: reference)
        }
    }

    // MARK: - MainActor reads

    private func listPodcastsSync(limit: Int) -> [PodcastInventoryRow] {
        agentInventory(op: "list_podcasts", limit: limit).podcasts.map(\.row)
    }

    private func listSubscriptionsSync(limit: Int) -> [SubscriptionSummary] {
        agentInventory(op: "list_subscriptions", limit: limit).subscriptions.map(\.row)
    }

    private func listEpisodesSync(podcastID: PodcastID, limit: Int) -> [EpisodeInventoryRow]? {
        let response = agentInventory(op: "list_episodes", limit: limit, podcastID: podcastID)
        if response.found == false { return nil }
        return response.episodes.map(\.row)
    }

    private func listInProgressSync(limit: Int) -> [EpisodeInventoryRow] {
        agentInventory(op: "list_in_progress", limit: limit).episodes.map(\.row)
    }

    private func listRecentUnplayedSync(limit: Int) -> [EpisodeInventoryRow] {
        agentInventory(op: "list_recent_unplayed", limit: limit).episodes.map(\.row)
    }

    private func listCategoriesSync(limit: Int, includePodcasts: Bool) -> [PodcastCategorySummary] {
        guard let store else { return [] }
        let projection = CategoryLibraryProjection.load(categories: store.state.categories, store: store)
        return categorySummaries(
            limit: limit,
            includePodcasts: includePodcasts,
            projection: projection,
            store: store
        )
    }

    private func changePodcastCategorySync(
        podcastID: PodcastID,
        category reference: PodcastCategoryReference
    ) throws -> PodcastCategoryChangeResult {
        guard let store else {
            throw PodcastAgentToolAdapterError.unavailable("AppStateStore")
        }
        let response = try planCategoryChange(
            podcastID: podcastID,
            reference: reference,
            store: store
        )
        store.setCategories(response.categories)
        store.kernel?.dispatch(namespace: "podcast",
                               body: [
                                   "op": "set_podcast_user_categories",
                                   "podcast_id": response.result.podcastID.lowercased(),
                                   "categories": response.labels,
                               ])
        return response.result
    }

    // MARK: - Helpers

    private func agentInventory(
        op: String,
        limit: Int,
        podcastID: PodcastID? = nil
    ) -> AgentInventoryEnvelope {
        guard let kernel = store?.kernel else { return AgentInventoryEnvelope() }
        var request: [String: Any] = ["op": op, "limit": limit]
        if let podcastID { request["podcast_id"] = podcastID }
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        guard let envelope = kernel.agentInventoryEnvelope(request: request),
              let data = envelope.data(using: .utf8),
              let decoded = try? decoder.decode(AgentInventoryEnvelope.self, from: data),
              decoded.error == nil
        else { return AgentInventoryEnvelope() }
        return decoded
    }

    private func categorySummaries(
        limit: Int,
        includePodcasts: Bool,
        projection: CategoryLibraryProjection,
        store: AppStateStore
    ) -> [PodcastCategorySummary] {
        let request: [String: Any] = [
            "op": "category_summaries",
            "args": [
                "limit": limit,
                "include_podcasts": includePodcasts,
            ],
            "categories": store.state.categories.map(Self.categoryRequestRow),
            "projected_categories": projection.categoryIDs.map { categoryID in
                categoryProjectionRow(categoryID: categoryID, projection: projection)
            },
            "podcasts": store.rustAllPodcasts().map(Self.categorySourcePodcastRow),
        ]
        guard let data = try? JSONSerialization.data(withJSONObject: request),
              let json = String(data: data, encoding: .utf8)
        else { return [] }
        return json.withCString { ptr -> [PodcastCategorySummary] in
            guard let result = nmp_app_podcast_agent_action_policy(ptr) else {
                return []
            }
            defer { nmp_free_string(result) }
            let envelope = String(cString: result)
            guard let data = envelope.data(using: .utf8),
                  let decoded = try? Self.categorySummariesDecoder.decode(
                    RustCategorySummariesResponse.self,
                    from: data
                  ),
                  decoded.error == nil
            else { return [] }
            return decoded.categories.map(\.summary)
        }
    }

    private func categoryProjectionRow(
        categoryID: UUID,
        projection: CategoryLibraryProjection
    ) -> [String: Any] {
        [
            "category_id": categoryID.uuidString,
            "podcast_ids": (projection.podcastIDsByCategory[categoryID] ?? []).map(\.uuidString),
        ]
    }

    private func planCategoryChange(
        podcastID: PodcastID,
        reference: PodcastCategoryReference,
        store: AppStateStore
    ) throws -> RustCategoryChangeResponse {
        var referencePayload: [String: Any] = [:]
        if let id = reference.id {
            referencePayload["id"] = id
        }
        if let slug = reference.slug {
            referencePayload["slug"] = slug
        }
        if let name = reference.name {
            referencePayload["name"] = name
        }
        let request: [String: Any] = [
            "podcast_id": podcastID,
            "reference": referencePayload,
            "categories": store.state.categories.map(Self.categoryRequestRow),
        ]
        guard let envelope = store.kernel?.libraryCategoryChangeEnvelope(request: request),
              let data = envelope.data(using: .utf8),
              let decoded = try? Self.categoryChangeDecoder.decode(RustCategoryChangeResponse.self, from: data)
        else {
            throw PodcastCategoryAdapterError.moveFailed
        }
        if let error = decoded.error {
            throw Self.mapCategoryChangeError(error, podcastID: podcastID)
        }
        guard let result = decoded.result else {
            throw PodcastCategoryAdapterError.moveFailed
        }
        return RustCategoryChangeResponse(
            error: nil,
            categories: decoded.categories,
            labels: decoded.labels,
            result: result
        )
    }

    private static func categoryRequestRow(_ category: PodcastCategory) -> [String: Any] {
        var row: [String: Any] = [
            "id": category.id.uuidString,
            "name": category.name,
            "slug": category.slug,
            "description": category.description,
            "subscription_ids": category.subscriptionIDs.map(\.uuidString),
            "generated_at": categoryDateFormatter.string(from: category.generatedAt),
        ]
        if let colorHex = category.colorHex {
            row["color_hex"] = colorHex
        }
        if let model = category.model {
            row["model"] = model
        }
        return row
    }

    private static func categorySourcePodcastRow(_ podcast: Podcast) -> [String: Any] {
        var row: [String: Any] = [
            "podcast_id": podcast.id.uuidString,
            "title": podcast.title,
        ]
        if !podcast.author.isEmpty {
            row["author"] = podcast.author
        }
        return row
    }

    private static func mapCategoryChangeError(
        _ error: String,
        podcastID: PodcastID
    ) -> Error {
        switch error {
        case "invalid_podcast_id":
            return PodcastAgentToolAdapterError.invalidID(podcastID)
        case "missing_podcast":
            return PodcastAgentToolAdapterError.missingPodcast(podcastID)
        case "missing_category":
            return PodcastCategoryAdapterError.missingCategory
        default:
            return PodcastCategoryAdapterError.moveFailed
        }
    }

    private static let categoryDateFormatter = ISO8601DateFormatter()

    private static let categoryChangeDecoder: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        decoder.dateDecodingStrategy = .iso8601
        return decoder
    }()

    private static let categorySummariesDecoder: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        decoder.dateDecodingStrategy = .iso8601
        return decoder
    }()
}

private struct RustCategorySummariesResponse: Decodable {
    let error: String?
    let categories: [RustCategorySummaryDTO]

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        error = try c.decodeIfPresent(String.self, forKey: .error)
        categories = try c.decodeIfPresent([RustCategorySummaryDTO].self, forKey: .categories) ?? []
    }
}

private struct RustCategorySummaryDTO: Decodable {
    let categoryId: String
    let name: String
    let slug: String
    let description: String
    let colorHex: String?
    let subscriptionCount: Int
    let generatedAt: Date
    let model: String?
    let subscriptions: [RustCategorySubscriptionSummaryDTO]

    var summary: PodcastCategorySummary {
        PodcastCategorySummary(
            categoryID: categoryId,
            name: name,
            slug: slug,
            description: description,
            colorHex: colorHex,
            subscriptionCount: subscriptionCount,
            generatedAt: generatedAt,
            model: model,
            subscriptions: subscriptions.map(\.summary)
        )
    }
}

private struct RustCategorySubscriptionSummaryDTO: Decodable {
    let podcastId: String
    let title: String
    let author: String?

    var summary: CategorySubscriptionSummary {
        CategorySubscriptionSummary(
            podcastID: podcastId,
            title: title,
            author: author
        )
    }
}

private struct RustCategoryChangeResponse: Decodable {
    let error: String?
    let categories: [PodcastCategory]
    let labels: [String]
    private let resultDTO: RustCategoryChangeResult?

    var result: PodcastCategoryChangeResult? {
        guard let resultDTO else { return nil }
        return PodcastCategoryChangeResult(
            podcastID: resultDTO.podcastID,
            title: resultDTO.title,
            previousCategoryID: resultDTO.previousCategoryID,
            previousCategoryName: resultDTO.previousCategoryName,
            categoryID: resultDTO.categoryID,
            categoryName: resultDTO.categoryName,
            categorySlug: resultDTO.categorySlug
        )
    }

    private enum CodingKeys: String, CodingKey {
        case error
        case categories
        case labels
        case resultDTO = "result"
    }

    init(
        error: String?,
        categories: [PodcastCategory],
        labels: [String],
        result: PodcastCategoryChangeResult
    ) {
        self.error = error
        self.categories = categories
        self.labels = labels
        self.resultDTO = RustCategoryChangeResult(
            podcastID: result.podcastID,
            title: result.title,
            previousCategoryID: result.previousCategoryID,
            previousCategoryName: result.previousCategoryName,
            categoryID: result.categoryID,
            categoryName: result.categoryName,
            categorySlug: result.categorySlug
        )
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        error = try container.decodeIfPresent(String.self, forKey: .error)
        categories = try container.decodeIfPresent([PodcastCategory].self, forKey: .categories) ?? []
        labels = try container.decodeIfPresent([String].self, forKey: .labels) ?? []
        resultDTO = try container.decodeIfPresent(RustCategoryChangeResult.self, forKey: .resultDTO)
    }
}

private struct RustCategoryChangeResult: Decodable {
    let podcastID: String
    let title: String
    let previousCategoryID: String?
    let previousCategoryName: String?
    let categoryID: String
    let categoryName: String
    let categorySlug: String
}

private struct AgentInventoryEnvelope: Decodable {
    var subscriptions: [AgentInventorySubscriptionDTO] = []
    var podcasts: [AgentInventoryPodcastDTO] = []
    var episodes: [AgentInventoryEpisodeDTO] = []
    var found: Bool?
    var error: String?

    init() {}

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        subscriptions = try c.decodeIfPresent([AgentInventorySubscriptionDTO].self, forKey: .subscriptions) ?? []
        podcasts = try c.decodeIfPresent([AgentInventoryPodcastDTO].self, forKey: .podcasts) ?? []
        episodes = try c.decodeIfPresent([AgentInventoryEpisodeDTO].self, forKey: .episodes) ?? []
        found = try c.decodeIfPresent(Bool.self, forKey: .found)
        error = try c.decodeIfPresent(String.self, forKey: .error)
    }
}

private struct AgentInventorySubscriptionDTO: Decodable {
    var podcastId: String
    var title: String
    var author: String?
    var totalEpisodes: Int
    var unplayedEpisodes: Int
    var lastPublishedAt: Int?

    var row: SubscriptionSummary {
        SubscriptionSummary(
            podcastID: podcastId,
            title: title,
            author: author,
            totalEpisodes: totalEpisodes,
            unplayedEpisodes: unplayedEpisodes,
            lastPublishedAt: lastPublishedAt.map { Date(timeIntervalSince1970: TimeInterval($0)) }
        )
    }
}

private struct AgentInventoryPodcastDTO: Decodable {
    var podcastId: String
    var title: String
    var author: String?
    var subscribed: Bool
    var totalEpisodes: Int
    var unplayedEpisodes: Int
    var lastPublishedAt: Int?

    var row: PodcastInventoryRow {
        PodcastInventoryRow(
            podcastID: podcastId,
            title: title,
            author: author,
            subscribed: subscribed,
            totalEpisodes: totalEpisodes,
            unplayedEpisodes: unplayedEpisodes,
            lastPublishedAt: lastPublishedAt.map { Date(timeIntervalSince1970: TimeInterval($0)) }
        )
    }
}

private struct AgentInventoryEpisodeDTO: Decodable {
    var episodeId: String
    var podcastId: String
    var title: String
    var podcastTitle: String
    var publishedAt: Int?
    var durationSeconds: Int?
    var played: Bool
    var playbackPositionSeconds: Double
    var isInProgress: Bool

    var row: EpisodeInventoryRow {
        EpisodeInventoryRow(
            episodeID: episodeId,
            podcastID: podcastId,
            title: title,
            podcastTitle: podcastTitle,
            publishedAt: publishedAt.map { Date(timeIntervalSince1970: TimeInterval($0)) },
            durationSeconds: durationSeconds,
            played: played,
            playbackPositionSeconds: playbackPositionSeconds,
            isInProgress: isInProgress
        )
    }
}

private enum PodcastCategoryAdapterError: LocalizedError {
    case missingCategory
    case moveFailed

    var errorDescription: String? {
        switch self {
        case .missingCategory:
            return "Category not found. Use list_categories to choose an existing category ID, slug, or name."
        case .moveFailed:
            return "Could not move podcast into the requested category."
        }
    }
}
