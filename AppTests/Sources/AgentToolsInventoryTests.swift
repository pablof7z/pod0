import XCTest
@testable import Podcastr

/// Coverage for the inventory/category podcast agent tools added to
/// `AgentTools.dispatchPodcast`:
///
///   - `list_subscriptions`
///   - `list_categories`
///   - `change_podcast_category`
///   - `list_episodes`
///   - `list_in_progress`
///   - `list_recent_unplayed`
///
/// These let the agent answer plain-English library questions ("what am I
/// subscribed to?", "what was I listening to?") without spending a search
/// or RAG call. Tests verify dispatch routing, default + capped limits,
/// missing-arg validation, and JSON shape.
@MainActor
final class AgentToolsInventoryTests: XCTestCase {

    // MARK: - list_subscriptions

    func testListSubscriptionsDispatchesAndReturnsAllRows() async throws {
        let inventory = MockInventory()
        await inventory.setSubscriptions([
            sampleSubscription(id: "p1", title: "Acquired"),
            sampleSubscription(id: "p2", title: "The Daily"),
        ])

        let result = await dispatch(name: "list_subscriptions", args: [:], inventory: inventory)

        let json = try unwrapJSON(result)
        XCTAssertEqual(json["success"] as? Bool, true)
        let rows = json["subscriptions"] as? [[String: Any]] ?? []
        XCTAssertEqual(rows.count, 2)
        XCTAssertEqual(rows.first?["title"] as? String, "Acquired")
        XCTAssertEqual(rows.first?["total_episodes"] as? Int, 100)
        XCTAssertEqual(rows.first?["unplayed_episodes"] as? Int, 5)
    }

    func testListSubscriptionsRespectsLimitArg() async throws {
        let inventory = MockInventory()
        await inventory.setSubscriptions((0..<10).map { sampleSubscription(id: "p\($0)", title: "Show \($0)") })

        _ = await dispatch(name: "list_subscriptions", args: ["limit": 3], inventory: inventory)

        let lastLimit = await inventory.lastListSubscriptionsLimit
        XCTAssertEqual(lastLimit, 3)
    }

    func testListSubscriptionsClampsLimitToCap() async throws {
        let inventory = MockInventory()

        _ = await dispatch(name: "list_subscriptions", args: ["limit": 999], inventory: inventory)

        // Capped at 100 (see AgentTools+Podcast inventoryMaxLimit).
        let lastLimit = await inventory.lastListSubscriptionsLimit
        XCTAssertEqual(lastLimit, 100)
    }

    func testListSubscriptionsDefaultsLimitWhenAbsent() async throws {
        let inventory = MockInventory()

        _ = await dispatch(name: "list_subscriptions", args: [:], inventory: inventory)

        let lastLimit = await inventory.lastListSubscriptionsLimit
        XCTAssertEqual(lastLimit, 25)
    }

    // MARK: - list_categories

    func testListCategoriesReturnsCategoryRowsAndPodcasts() async throws {
        let inventory = MockInventory()
        await inventory.setCategories([
            sampleCategory(id: "c1", name: "Deep Tech", subscriptions: [
                CategorySubscriptionSummary(podcastID: "p1", title: "Acquired", author: "Ben and David"),
            ]),
        ])

        let result = await dispatch(name: "list_categories", args: [:], inventory: inventory)

        let json = try unwrapJSON(result)
        XCTAssertEqual(json["success"] as? Bool, true)
        let rows = json["categories"] as? [[String: Any]] ?? []
        XCTAssertEqual(rows.count, 1)
        XCTAssertEqual(rows.first?["name"] as? String, "Deep Tech")
        XCTAssertEqual(rows.first?["subscription_count"] as? Int, 1)
        let subscriptions = rows.first?["subscriptions"] as? [[String: Any]] ?? []
        XCTAssertEqual(subscriptions.first?["title"] as? String, "Acquired")
    }

    func testListCategoriesCanOmitPodcastRows() async throws {
        let inventory = MockInventory()
        await inventory.setCategories([sampleCategory(id: "c1", name: "News")])

        let result = await dispatch(
            name: "list_categories",
            args: ["include_podcasts": false, "limit": 7],
            inventory: inventory
        )

        let json = try unwrapJSON(result)
        let rows = json["categories"] as? [[String: Any]] ?? []
        XCTAssertNil(rows.first?["subscriptions"])
        let lastLimit = await inventory.lastListCategoriesLimit
        let include = await inventory.lastListCategoriesIncludePodcasts
        XCTAssertEqual(lastLimit, 7)
        XCTAssertEqual(include, false)
    }

    // MARK: - change_podcast_category

    func testChangePodcastCategoryRequiresPodcastID() async throws {
        let result = await dispatch(
            name: "change_podcast_category",
            args: ["category_slug": "news"],
            inventory: MockInventory()
        )
        let json = try unwrapJSON(result)
        XCTAssertNotNil(json["error"])
    }

    func testChangePodcastCategoryForwardsSlugReference() async throws {
        let inventory = MockInventory()
        await inventory.setCategoryChangeResult(PodcastCategoryChangeResult(
            podcastID: "p1",
            title: "The Daily",
            previousCategoryID: "old",
            previousCategoryName: "Politics",
            categoryID: "new",
            categoryName: "News",
            categorySlug: "news"
        ))

        let result = await dispatch(
            name: "change_podcast_category",
            args: ["podcast_id": "p1", "category_slug": "news"],
            inventory: inventory
        )

        let json = try unwrapJSON(result)
        XCTAssertEqual(json["success"] as? Bool, true)
        XCTAssertEqual(json["category_slug"] as? String, "news")
        let lastPodcast = await inventory.lastCategoryChangePodcastID
        let lastReference = await inventory.lastCategoryChangeReference
        XCTAssertEqual(lastPodcast, "p1")
        XCTAssertEqual(lastReference?.slug, "news")
    }

    // `list_episodes` tests live in `AgentToolsListEpisodesTests.swift` so this
    // file stays under the 500-line cap from `AGENTS.md`.

    // MARK: - list_in_progress

    func testListInProgressDispatchesWithDefaultLimit() async throws {
        let inventory = MockInventory()
        await inventory.setInProgress([
            sampleEpisode(id: "e1", podcast: "p1", played: false, position: 600),
        ])

        let result = await dispatch(name: "list_in_progress", args: [:], inventory: inventory)

        let json = try unwrapJSON(result)
        XCTAssertEqual(json["success"] as? Bool, true)
        let rows = json["episodes"] as? [[String: Any]] ?? []
        XCTAssertEqual(rows.count, 1)
        let lastLimit = await inventory.lastInProgressLimit
        XCTAssertEqual(lastLimit, 25)
    }

    // MARK: - list_recent_unplayed

    func testListRecentUnplayedDispatches() async throws {
        let inventory = MockInventory()
        await inventory.setRecentUnplayed([
            sampleEpisode(id: "e1", podcast: "p1", played: false, position: 0),
            sampleEpisode(id: "e2", podcast: "p1", played: false, position: 0),
        ])

        let result = await dispatch(name: "list_recent_unplayed", args: ["limit": 5], inventory: inventory)

        let json = try unwrapJSON(result)
        XCTAssertEqual(json["success"] as? Bool, true)
        let rows = json["episodes"] as? [[String: Any]] ?? []
        XCTAssertEqual(rows.count, 2)
        let lastLimit = await inventory.lastRecentUnplayedLimit
        XCTAssertEqual(lastLimit, 5)
    }

    // MARK: - Schema includes the new tools

    func testSchemaExposesAllFourInventoryTools() {
        let names = AgentTools.podcastSchema
            .compactMap { ($0["function"] as? [String: Any])?["name"] as? String }
        XCTAssertTrue(names.contains("list_subscriptions"))
        XCTAssertTrue(names.contains("list_categories"))
        XCTAssertTrue(names.contains("change_podcast_category"))
        XCTAssertTrue(names.contains("list_episodes"))
        XCTAssertTrue(names.contains("list_in_progress"))
        XCTAssertTrue(names.contains("list_recent_unplayed"))
    }

    func testPodcastNamesAllIncludesInventoryTools() {
        let all = AgentTools.PodcastNames.all
        XCTAssertTrue(all.contains("list_subscriptions"))
        XCTAssertTrue(all.contains("list_categories"))
        XCTAssertTrue(all.contains("change_podcast_category"))
        XCTAssertTrue(all.contains("list_episodes"))
        XCTAssertTrue(all.contains("list_in_progress"))
        XCTAssertTrue(all.contains("list_recent_unplayed"))
    }

    // MARK: - Helpers

    private func dispatch(
        name: String,
        args: [String: Any],
        inventory: MockInventory,
        directory: MockDirectory = MockDirectory(),
        subscribe: MockSubscribe = MockSubscribe()
    ) async -> String {
        let deps = PodcastAgentToolDeps(
            rag: MockRAG(),
            wiki: MockWiki(),
            briefing: MockBriefing(),
            summarizer: MockSummarizer(),
            fetcher: MockFetcher(),
            playback: MockPlayback(),
            library: MockLibrary(),
            inventory: inventory,
            categories: inventory,
            peerPublisher: MockPeerEventPublisher(),
            friendDirectory: MockFriendDirectory(),
            perplexity: MockPerplexity(),
            ttsPublisher: MockTTSPublisher(),
            directory: directory,
            subscribe: subscribe,
            youtubeIngestion: MockYouTubeIngestion(),
            ownedPodcasts: MockOwnedPodcasts()
        )
        // Round-trip through the JSON-string dispatcher so we don't have to
        // hand a non-Sendable `[String: Any]` across isolation boundaries.
        let argsJSON: String
        if args.isEmpty {
            argsJSON = "{}"
        } else {
            let data = (try? JSONSerialization.data(withJSONObject: args)) ?? Data("{}".utf8)
            argsJSON = String(data: data, encoding: .utf8) ?? "{}"
        }
        return await AgentTools.dispatchPodcast(name: name, argsJSON: argsJSON, deps: deps)
    }

    private func unwrapJSON(_ result: String) throws -> [String: Any] {
        let data = try XCTUnwrap(result.data(using: .utf8))
        return try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
    }

    private func sampleSubscription(id: String, title: String) -> SubscriptionSummary {
        SubscriptionSummary(
            podcastID: id,
            title: title,
            author: "Test Author",
            totalEpisodes: 100,
            unplayedEpisodes: 5,
            lastPublishedAt: Date(timeIntervalSince1970: 1_700_000_000)
        )
    }

    private func sampleCategory(
        id: String,
        name: String,
        subscriptions: [CategorySubscriptionSummary] = []
    ) -> PodcastCategorySummary {
        PodcastCategorySummary(
            categoryID: id,
            name: name,
            slug: name.lowercased().replacingOccurrences(of: " ", with: "-"),
            description: "Shows about \(name)",
            colorHex: "#5B8DEF",
            subscriptionCount: subscriptions.count,
            generatedAt: Date(timeIntervalSince1970: 1_700_000_000),
            model: "test",
            subscriptions: subscriptions
        )
    }

    private func sampleEpisode(
        id: String,
        podcast: String,
        played: Bool,
        position: Double
    ) -> EpisodeInventoryRow {
        EpisodeInventoryRow(
            episodeID: id,
            podcastID: podcast,
            title: "Episode \(id)",
            podcastTitle: "Show \(podcast)",
            publishedAt: Date(timeIntervalSince1970: 1_700_000_000),
            durationSeconds: 1800,
            played: played,
            playbackPositionSeconds: position,
            isInProgress: !played && position > 0
        )
    }
}
