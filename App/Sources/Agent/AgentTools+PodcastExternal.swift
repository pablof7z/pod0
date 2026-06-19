import Foundation

// MARK: - External podcast tool handlers
//
// Handlers for the external-podcast tools:
//   • search_podcast_directory  — iTunes Search API
//   • subscribe_podcast         — subscribe to a feed by RSS URL
//   • download_and_transcribe (external path) — auto-subscribe then transcribe
//
// External playback (audio_url + title path) is handled by
// `playExternalAudioURL` inside `AgentTools+PodcastActions.swift`, dispatched
// by the unified `play_episode` tool.

extension AgentTools {
    private struct DirectorySearchPlan: Decodable {
        let error: String?
        let query: String?
        let searchType: String?
        let limit: Int?

        enum CodingKeys: String, CodingKey {
            case error, query, limit
            case searchType = "search_type"
        }
    }

    private struct ExternalPodcastActionPlan: Decodable {
        let error: String?
        let feedURL: String?
        let podcastID: String?

        enum CodingKeys: String, CodingKey {
            case error
            case feedURL = "feed_url"
            case podcastID = "podcast_id"
        }
    }

    // MARK: - Limits

    static let directorySearchDefaultLimit = 5
    static let directorySearchMaxLimit = 20

    // MARK: - search_podcast_directory

    static func searchPodcastDirectoryTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        let planEnvelope = await directorySearchPlanEnvelope(args: args)
        guard let planEnvelope,
              let planData = planEnvelope.data(using: .utf8),
              let plan = try? JSONDecoder().decode(DirectorySearchPlan.self, from: planData)
        else { return toolError("Podcast directory search planning is unavailable") }
        if let error = plan.error { return toolError(error) }
        guard let query = plan.query, let searchType = plan.searchType, let limit = plan.limit else {
            return toolError("Podcast directory search plan was incomplete")
        }
        let type: PodcastDirectorySearchType = searchType == "podcast" ? .podcast : .episode
        do {
            let hits = try await deps.directory.searchDirectory(query: query, type: type, limit: limit)
            return await directorySearchResultsEnvelope(query: query, searchType: searchType, hits: hits)
                ?? toolError("Podcast directory search result shaping is unavailable")
        } catch {
            return toolError("search_podcast_directory failed: \(error.localizedDescription)")
        }
    }

    // MARK: - unfollow_podcast

    static func unfollowPodcastTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let plan = await externalPodcastActionPlan(op: "unfollow_podcast_plan", args: args) else {
            return toolError("unfollow_podcast planning is unavailable")
        }
        if let error = plan.error { return toolError(error) }
        guard let podcastID = plan.podcastID else { return toolError("unfollow_podcast plan was incomplete") }
        do {
            let result = try await deps.subscribe.unfollowPodcast(podcastID: podcastID)
            var payload: [String: Any] = [
                "podcast_id": result.podcastID,
                "was_subscribed": result.wasSubscribed,
            ]
            if let title = result.title { payload["title"] = title }
            return await actionTool(op: "unfollow_podcast_result", payload: payload)
                ?? toolError("unfollow_podcast result shaping is unavailable")
        } catch {
            return toolError("unfollow_podcast failed: \(error.localizedDescription)")
        }
    }

    // MARK: - delete_podcast

    static func deletePodcastTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let plan = await externalPodcastActionPlan(op: "delete_podcast_plan", args: args) else {
            return toolError("delete_podcast planning is unavailable")
        }
        if let error = plan.error { return toolError(error) }
        guard let podcastID = plan.podcastID else { return toolError("delete_podcast plan was incomplete") }
        do {
            let result = try await deps.subscribe.deletePodcast(podcastID: podcastID)
            var payload: [String: Any] = [
                "podcast_id": result.podcastID,
                "was_subscribed": result.wasSubscribed,
                "episodes_deleted": result.episodesDeleted,
            ]
            if let title = result.title { payload["title"] = title }
            return await actionTool(op: "delete_podcast_result", payload: payload)
                ?? toolError("delete_podcast result shaping is unavailable")
        } catch {
            return toolError("delete_podcast failed: \(error.localizedDescription)")
        }
    }

    // MARK: - subscribe_podcast

    static func subscribePodcastTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let plan = await externalPodcastActionPlan(op: "subscribe_plan", args: args) else {
            return toolError("subscribe_podcast planning is unavailable")
        }
        if let error = plan.error { return toolError(error) }
        guard let feedURL = plan.feedURL else { return toolError("subscribe_podcast plan was incomplete") }
        do {
            let result = try await deps.subscribe.subscribe(feedURLString: feedURL)
            var payload: [String: Any] = [
                "podcast_id": result.podcastID,
                "title": result.title,
                "feed_url": result.feedURL,
                "episode_count": result.episodeCount,
                "already_subscribed": result.alreadySubscribed,
            ]
            if let author = result.author { payload["author"] = author }
            return await actionTool(op: "subscribe_result", payload: payload)
                ?? toolError("subscribe_podcast result shaping is unavailable")
        } catch {
            return toolError("subscribe_podcast failed: \(error.localizedDescription)")
        }
    }

    // MARK: - download_and_transcribe (external path)
    //
    // Called from AgentTools+PodcastActions.downloadAndTranscribeTool when
    // the args contain audio_url + feed_url but no episode_id.

    static func downloadAndTranscribeExternalTool(
        feedURLString: String,
        audioURLString: String,
        deps: PodcastAgentToolDeps
    ) async -> String {
        // Step 1: capture the feed (metadata + episodes) WITHOUT flipping the
        // user's subscription bit. The external download path used to call
        // `subscribe` here, which silently followed shows as a side effect of
        // a transcript request — that contradicted the Podcast/PodcastSubscription
        // split. Use `ensurePodcast` so the show lands in the library but the
        // user has to opt in to subscribing separately.
        let ensured: PodcastEnsureResult
        do {
            ensured = try await deps.subscribe.ensurePodcast(feedURLString: feedURLString)
        } catch {
            return toolError("Could not load feed '\(feedURLString)': \(error.localizedDescription)")
        }

        // Step 2: locate the episode by matching audio URL.
        guard let matchedID = await deps.fetcher.episodeIDForAudioURL(audioURLString, podcastID: ensured.podcastID) else {
            return toolError("""
            Episode with audio_url '\(audioURLString)' not found in the feed after loading it. \
            Try refresh_feed(podcast_id: '\(ensured.podcastID)') then list_episodes to locate the episode manually.
            """)
        }

        // Step 3: download and transcribe via the normal path.
        do {
            let result = try await deps.library.downloadAndTranscribe(episodeID: matchedID)
            var payload: [String: Any] = [
                "episode_id": result.episodeID,
                "podcast_id": ensured.podcastID,
                "podcast_title": ensured.title,
                "status": result.status,
            ]
            if let source = result.source { payload["source"] = source }
            if let message = result.message { payload["message"] = message }
            return await actionTool(op: "transcript_result", payload: payload)
                ?? toolError("download_and_transcribe result shaping is unavailable")
        } catch {
            return toolError("download_and_transcribe failed: \(error.localizedDescription)")
        }
    }

    // MARK: - Directory search FFI helpers

    private static func directorySearchPlanEnvelope(args: [String: Any]) async -> String? {
        await directorySearchFFI(payload: ["args": args], op: "plan")
    }

    private static func directorySearchResultsEnvelope(
        query: String,
        searchType: String,
        hits: [PodcastDirectoryHit]
    ) async -> String? {
        await directorySearchFFI(
            payload: [
                "query": query,
                "search_type": searchType,
                "results": hits.map(rawDirectoryHit),
            ],
            op: "results"
        )
    }

    private static func rawDirectoryHit(_ hit: PodcastDirectoryHit) -> [String: Any] {
        var row: [String: Any] = [
            "podcast_title": hit.podcastTitle,
        ]
        if let id = hit.collectionID { row["collection_id"] = id }
        if let author = hit.author { row["author"] = author }
        if let feedURL = hit.feedURL { row["feed_url"] = feedURL }
        if let artworkURL = hit.artworkURL { row["artwork_url"] = artworkURL }
        if let episodeTitle = hit.episodeTitle { row["episode_title"] = episodeTitle }
        if let audioURL = hit.episodeAudioURL { row["audio_url"] = audioURL }
        if let guid = hit.episodeGUID { row["episode_guid"] = guid }
        if let publishedAt = hit.episodePublishedAt {
            row["published_at"] = Int(publishedAt.timeIntervalSince1970)
        }
        if let duration = hit.episodeDurationSeconds { row["duration_seconds"] = duration }
        if let description = hit.episodeDescription { row["description"] = description }
        return row
    }

    private static func directorySearchFFI(payload: [String: Any], op: String) async -> String? {
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
                    result = nmp_app_podcast_agent_directory_search_plan(handle, ptr)
                case "results":
                    result = nmp_app_podcast_agent_directory_search_results(handle, ptr)
                default:
                    result = nil
                }
                guard let result else { return nil }
                defer { nmp_free_string(result) }
                return String(cString: result)
            }
        }.value
    }

    private static func externalPodcastActionPlan(
        op: String,
        args: [String: Any]
    ) async -> ExternalPodcastActionPlan? {
        guard let envelope = await actionTool(op: op, payload: args),
              let data = envelope.data(using: .utf8)
        else { return nil }
        return try? JSONDecoder().decode(ExternalPodcastActionPlan.self, from: data)
    }
}
