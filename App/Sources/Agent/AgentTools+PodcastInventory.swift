import Foundation

// MARK: - Podcast inventory tools (lane 10 — library queries)
//
// The four "what's in my library?" tools — `list_subscriptions`,
// `list_episodes`, `list_in_progress`, `list_recent_unplayed`. These let
// the agent answer plain-English questions about the user's existing
// library state without spending a search or RAG call. Split out of
// `AgentTools+Podcast.swift` to keep that file under the 500-line hard
// limit set by `AGENTS.md`.

extension AgentTools {
    private struct CategoryChangePlan: Decodable {
        let error: String?
        let podcastID: String?
        let categoryID: String?
        let categorySlug: String?
        let categoryName: String?

        enum CodingKeys: String, CodingKey {
            case error
            case podcastID = "podcast_id"
            case categoryID = "category_id"
            case categorySlug = "category_slug"
            case categoryName = "category_name"
        }
    }

    private struct EpisodeListPlan: Decodable {
        let error: String?
        let source: String?
        let podcastID: String?
        let feedURL: String?
        let collectionID: String?
        let limit: Int?

        enum CodingKeys: String, CodingKey {
            case error, source, limit
            case podcastID = "podcast_id"
            case feedURL = "feed_url"
            case collectionID = "collection_id"
        }
    }

    /// Shared formatter for the inventory serializers. `ISO8601DateFormatter`
    /// is expensive to allocate (touches Foundation locale tables) and is
    /// thread-safe for reads after construction, so the per-row allocations
    /// in `serializeCategory` / `serializeInventoryRow` / `listSubscriptionsTool`
    /// were pure waste — a 200-episode `list_episodes` response was minting
    /// 200 formatters and discarding them. One shared instance is reused
    /// across calls.
    nonisolated(unsafe) private static let iso8601 = ISO8601DateFormatter()

    // MARK: - Tool dispatch entry points

    static func listPodcastsTool(
        args: [String: Any],
        deps: PodcastAgentToolDeps
    ) async -> String {
        let rows = await deps.inventory.listPodcasts(limit: inventoryMaxLimit)
        return await inventoryListEnvelope(
            op: "list_podcasts",
            args: args,
            podcasts: rows.map(rawPodcastRow)
        ) ?? toolError("Inventory list shaping is unavailable")
    }

    static func listSubscriptionsTool(
        args: [String: Any],
        deps: PodcastAgentToolDeps
    ) async -> String {
        let rows = await deps.inventory.listSubscriptions(limit: inventoryMaxLimit)
        return await inventoryListEnvelope(
            op: "list_subscriptions",
            args: args,
            subscriptions: rows.map(rawSubscriptionRow)
        ) ?? toolError("Inventory list shaping is unavailable")
    }

    static func listCategoriesTool(
        args: [String: Any],
        deps: PodcastAgentToolDeps
    ) async -> String {
        let categories = await deps.categories.listCategories(
            limit: inventoryMaxLimit,
            includePodcasts: true
        )
        return await categoryListEnvelope(args: args, categories: categories)
            ?? toolError("Category list shaping is unavailable")
    }

    static func changePodcastCategoryTool(
        args: [String: Any],
        deps: PodcastAgentToolDeps
    ) async -> String {
        guard let plan = await categoryChangePlan(args: args) else {
            return toolError("change_podcast_category planning is unavailable")
        }
        if let error = plan.error { return toolError(error) }
        guard let podcastID = plan.podcastID else {
            return toolError("change_podcast_category plan was incomplete")
        }
        let reference = PodcastCategoryReference(
            id: plan.categoryID,
            slug: plan.categorySlug,
            name: plan.categoryName
        )
        do {
            let result = try await deps.categories.changePodcastCategory(
                podcastID: podcastID,
                category: reference
            )
            return await actionTool(op: "category_change_result", payload: rawCategoryChange(result))
                ?? toolError("change_podcast_category result shaping is unavailable")
        } catch {
            return toolError("change_podcast_category failed: \(error.localizedDescription)")
        }
    }

    static func listEpisodesTool(
        args: [String: Any],
        deps: PodcastAgentToolDeps
    ) async -> String {
        guard let plan = await episodeListPlan(args: args) else {
            return toolError("Episode list planning is unavailable")
        }
        if let error = plan.error { return toolError(error) }
        guard let source = plan.source, let limit = plan.limit else {
            return toolError("Episode list plan was incomplete")
        }
        switch source {
        case "internal":
            guard let podcastID = plan.podcastID else {
                return await episodeListError(kind: "unknown", detail: "Episode list plan was incomplete")
            }
            guard let rows = await deps.inventory.listEpisodes(podcastID: podcastID, limit: limit) else {
                return await episodeListError(kind: "unknown_podcast", podcastID: podcastID)
            }
            return await episodeListResults(
                source: source,
                podcastID: podcastID,
                podcastTitle: rows.first?.podcastTitle,
                rows: rows
            )
        case "feed_url":
            guard let feedURL = plan.feedURL else {
                return await episodeListError(kind: "unknown", detail: "Episode list plan was incomplete")
            }
            return await listEpisodesFromFeedURL(feedURL, limit: limit, source: source, deps: deps)
        case "collection_id":
            guard let collectionID = plan.collectionID else {
                return await episodeListError(kind: "unknown", detail: "Episode list plan was incomplete")
            }
            return await listEpisodesFromCollectionID(collectionID, limit: limit, deps: deps)
        default:
            return await episodeListError(kind: "unknown", detail: "Episode list plan was incomplete")
        }
    }

    /// External path: resolve a directory collection ID → feed URL, then
    /// hand off to the feed-URL path.
    private static func listEpisodesFromCollectionID(
        _ collectionID: String,
        limit: Int,
        deps: PodcastAgentToolDeps
    ) async -> String {
        let feedURL: String?
        do {
            feedURL = try await deps.directory.lookupFeedURL(forCollectionID: collectionID)
        } catch {
            return await episodeListError(
                kind: "collection_lookup_failed",
                collectionID: collectionID,
                detail: error.localizedDescription
            )
        }
        guard let feedURL else {
            return await episodeListError(kind: "collection_not_found", collectionID: collectionID)
        }
        return await listEpisodesFromFeedURL(feedURL, limit: limit, source: "collection_id", deps: deps)
    }

    /// External path: ensure the feed is captured (metadata + episodes,
    /// without subscribing), then read episodes back via the inventory
    /// adapter so the response shape matches the internal path.
    private static func listEpisodesFromFeedURL(
        _ feedURL: String,
        limit: Int,
        source: String,
        deps: PodcastAgentToolDeps
    ) async -> String {
        let ensured: PodcastEnsureResult
        do {
            ensured = try await deps.subscribe.ensurePodcast(feedURLString: feedURL)
        } catch {
            return await episodeListError(
                kind: "feed_load_failed",
                feedURL: feedURL,
                detail: error.localizedDescription
            )
        }
        guard let rows = await deps.inventory.listEpisodes(
            podcastID: ensured.podcastID,
            limit: limit
        ) else {
            return await episodeListError(kind: "feed_row_missing", feedURL: feedURL)
        }
        return await episodeListResults(
            source: source,
            podcastID: ensured.podcastID,
            feedURL: ensured.feedURL,
            podcastTitle: ensured.title,
            rows: rows
        )
    }

    static func listInProgressTool(
        args: [String: Any],
        deps: PodcastAgentToolDeps
    ) async -> String {
        let rows = await deps.inventory.listInProgress(limit: inventoryMaxLimit)
        return await inventoryListEnvelope(
            op: "list_in_progress",
            args: args,
            episodes: rows.map(rawEpisodeRow)
        ) ?? toolError("Inventory list shaping is unavailable")
    }

    static func listRecentUnplayedTool(
        args: [String: Any],
        deps: PodcastAgentToolDeps
    ) async -> String {
        let rows = await deps.inventory.listRecentUnplayed(limit: inventoryMaxLimit)
        return await inventoryListEnvelope(
            op: "list_recent_unplayed",
            args: args,
            episodes: rows.map(rawEpisodeRow)
        ) ?? toolError("Inventory list shaping is unavailable")
    }

    // MARK: - Helpers

    private static let inventoryDefaultLimit = 25
    private static let inventoryMaxLimit = 100

    static func clampedInventoryLimit(_ raw: Any?) -> Int {
        guard let n = numericArg(raw) else { return inventoryDefaultLimit }
        return max(1, min(Int(n), inventoryMaxLimit))
    }

    static func rawCategoryChange(_ result: PodcastCategoryChangeResult) -> [String: Any] {
        var out: [String: Any] = [
            "podcast_id": result.podcastID,
            "title": result.title,
            "category_id": result.categoryID,
            "category_name": result.categoryName,
            "category_slug": result.categorySlug,
        ]
        if let id = result.previousCategoryID { out["previous_category_id"] = id }
        if let name = result.previousCategoryName { out["previous_category_name"] = name }
        return out
    }

    private static func categoryChangePlan(args: [String: Any]) async -> CategoryChangePlan? {
        guard let envelope = await actionTool(op: "category_change_plan", payload: args),
              let data = envelope.data(using: .utf8)
        else { return nil }
        return try? JSONDecoder().decode(CategoryChangePlan.self, from: data)
    }

    private static func categoryListEnvelope(
        args: [String: Any],
        categories: [PodcastCategorySummary]
    ) async -> String? {
        let handleBits = await MainActor.run {
            KernelModel.shared?.podcastHandlePointer.map { Int(bitPattern: $0) }
        }
        guard let handleBits else { return nil }
        let request: [String: Any] = [
            "args": args,
            "categories": categories.map(rawCategoryRow),
        ]
        guard let data = try? JSONSerialization.data(withJSONObject: request),
              let json = String(data: data, encoding: .utf8)
        else { return nil }
        return await Task.detached(priority: .userInitiated) {
            guard let handle = UnsafeMutableRawPointer(bitPattern: handleBits) else {
                return nil
            }
            return json.withCString { ptr in
                guard let result = nmp_app_podcast_agent_category_list(handle, ptr) else {
                    return nil
                }
                defer { nmp_free_string(result) }
                return String(cString: result)
            }
        }.value
    }

    private static func rawCategoryRow(_ category: PodcastCategorySummary) -> [String: Any] {
        var row: [String: Any] = [
            "category_id": category.categoryID,
            "name": category.name,
            "slug": category.slug,
            "description": category.description,
            "subscription_count": category.subscriptionCount,
            "generated_at": Int(category.generatedAt.timeIntervalSince1970),
            "subscriptions": category.subscriptions.map { subscription in
                var sub: [String: Any] = [
                    "podcast_id": subscription.podcastID,
                    "title": subscription.title,
                ]
                if let author = subscription.author { sub["author"] = author }
                return sub
            },
        ]
        if let colorHex = category.colorHex { row["color_hex"] = colorHex }
        if let model = category.model { row["model"] = model }
        return row
    }

    private static func inventoryListEnvelope(
        op: String,
        args: [String: Any],
        podcasts: [[String: Any]] = [],
        subscriptions: [[String: Any]] = [],
        episodes: [[String: Any]] = []
    ) async -> String? {
        let handleBits = await MainActor.run {
            KernelModel.shared?.podcastHandlePointer.map { Int(bitPattern: $0) }
        }
        guard let handleBits else { return nil }
        let request: [String: Any] = [
            "op": op,
            "args": args,
            "podcasts": podcasts,
            "subscriptions": subscriptions,
            "episodes": episodes,
        ]
        guard let data = try? JSONSerialization.data(withJSONObject: request),
              let json = String(data: data, encoding: .utf8)
        else { return nil }
        return await Task.detached(priority: .userInitiated) {
            guard let handle = UnsafeMutableRawPointer(bitPattern: handleBits) else {
                return nil
            }
            return json.withCString { ptr in
                guard let result = nmp_app_podcast_agent_inventory_list(handle, ptr) else {
                    return nil
                }
                defer { nmp_free_string(result) }
                return String(cString: result)
            }
        }.value
    }

    private static func rawPodcastRow(_ row: PodcastInventoryRow) -> [String: Any] {
        var out: [String: Any] = [
            "podcast_id": row.podcastID,
            "title": row.title,
            "subscribed": row.subscribed,
            "total_episodes": row.totalEpisodes,
            "unplayed_episodes": row.unplayedEpisodes,
        ]
        if let author = row.author { out["author"] = author }
        if let date = row.lastPublishedAt { out["last_published_at"] = Int(date.timeIntervalSince1970) }
        return out
    }

    private static func rawSubscriptionRow(_ row: SubscriptionSummary) -> [String: Any] {
        var out: [String: Any] = [
            "podcast_id": row.podcastID,
            "title": row.title,
            "total_episodes": row.totalEpisodes,
            "unplayed_episodes": row.unplayedEpisodes,
        ]
        if let author = row.author { out["author"] = author }
        if let date = row.lastPublishedAt { out["last_published_at"] = Int(date.timeIntervalSince1970) }
        return out
    }

    private static func rawEpisodeRow(_ row: EpisodeInventoryRow) -> [String: Any] {
        var out: [String: Any] = [
            "episode_id": row.episodeID,
            "podcast_id": row.podcastID,
            "title": row.title,
            "podcast_title": row.podcastTitle,
            "played": row.played,
            "playback_position_seconds": row.playbackPositionSeconds,
            "is_in_progress": row.isInProgress,
        ]
        if let date = row.publishedAt { out["published_at"] = Int(date.timeIntervalSince1970) }
        if let duration = row.durationSeconds { out["duration_seconds"] = duration }
        return out
    }

    private static func episodeListPlan(args: [String: Any]) async -> EpisodeListPlan? {
        guard let envelope = await episodeListFFI(payload: ["args": args], op: "plan"),
              let data = envelope.data(using: .utf8)
        else { return nil }
        return try? JSONDecoder().decode(EpisodeListPlan.self, from: data)
    }

    private static func episodeListResults(
        source: String,
        podcastID: String,
        feedURL: String? = nil,
        podcastTitle: String? = nil,
        rows: [EpisodeInventoryRow]
    ) async -> String {
        var payload: [String: Any] = [
            "source": source,
            "podcast_id": podcastID,
            "episodes": rows.map(rawEpisodeRow),
        ]
        if let feedURL { payload["feed_url"] = feedURL }
        if let podcastTitle { payload["podcast_title"] = podcastTitle }
        return await episodeListFFI(payload: payload, op: "results")
            ?? toolError("Episode list result shaping is unavailable")
    }

    private static func episodeListError(
        kind: String,
        podcastID: String? = nil,
        feedURL: String? = nil,
        collectionID: String? = nil,
        detail: String? = nil
    ) async -> String {
        var payload: [String: Any] = ["kind": kind]
        if let podcastID { payload["podcast_id"] = podcastID }
        if let feedURL { payload["feed_url"] = feedURL }
        if let collectionID { payload["collection_id"] = collectionID }
        if let detail { payload["detail"] = detail }
        return await episodeListFFI(payload: payload, op: "error")
            ?? toolError(detail ?? "Episode list failed")
    }

    private static func episodeListFFI(payload: [String: Any], op: String) async -> String? {
        let handleBits = await MainActor.run {
            KernelModel.shared?.podcastHandlePointer.map { Int(bitPattern: $0) }
        }
        guard let handleBits,
              let data = try? JSONSerialization.data(withJSONObject: payload),
              let json = String(data: data, encoding: .utf8)
        else { return nil }
        return await Task.detached(priority: .userInitiated) {
            guard let handle = UnsafeMutableRawPointer(bitPattern: handleBits) else {
                return nil
            }
            return json.withCString { ptr in
                let result: UnsafeMutablePointer<CChar>?
                switch op {
                case "plan":
                    result = nmp_app_podcast_agent_episode_list_plan(handle, ptr)
                case "results":
                    result = nmp_app_podcast_agent_episode_list_results(handle, ptr)
                case "error":
                    result = nmp_app_podcast_agent_episode_list_error(handle, ptr)
                default:
                    result = nil
                }
                guard let result else { return nil }
                defer { nmp_free_string(result) }
                return String(cString: result)
            }
        }.value
    }

    static func serializeInventoryRow(_ row: EpisodeInventoryRow) -> [String: Any] {
        var out: [String: Any] = [
            "episode_id": row.episodeID,
            "podcast_id": row.podcastID,
            "title": row.title,
            "podcast_title": row.podcastTitle,
            "played": row.played,
            "playback_position_seconds": row.playbackPositionSeconds,
            "is_in_progress": row.isInProgress,
        ]
        if let date = row.publishedAt {
            out["published_at"] = Self.iso8601.string(from: date)
        }
        if let dur = row.durationSeconds { out["duration_seconds"] = dur }
        return out
    }
}
