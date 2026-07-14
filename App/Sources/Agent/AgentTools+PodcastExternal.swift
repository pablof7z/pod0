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

    // MARK: - Limits

    static let directorySearchDefaultLimit = 5
    static let directorySearchMaxLimit = 20

    // MARK: - search_podcast_directory

    static func searchPodcastDirectoryTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let query = (args["query"] as? String)?.trimmed, !query.isEmpty else {
            return toolError("Missing or empty 'query'")
        }
        let typeRaw = (args["type"] as? String)?.trimmed.lowercased() ?? "episode"
        let type: PodcastDirectorySearchType = typeRaw == "podcast" ? .podcast : .episode
        let limit = clampedDirectoryLimit(args["limit"])
        do {
            let hits = try await deps.directory.searchDirectory(query: query, type: type, limit: limit)
            let rows = hits.map(serializeDirectoryHit)
            return toolSuccess([
                "query": query,
                "type": typeRaw,
                "total_found": rows.count,
                "results": rows,
            ])
        } catch {
            return toolError("search_podcast_directory failed: \(error.localizedDescription)")
        }
    }

    // MARK: - delete_podcast

    static func deletePodcastTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let podcastID = (args["podcast_id"] as? String)?.trimmed, !podcastID.isEmpty else {
            return toolError("Missing or empty 'podcast_id'")
        }
        do {
            let result = try await deps.subscribe.deletePodcast(podcastID: podcastID)
            var payload: [String: Any] = [
                "podcast_id": result.podcastID,
                "was_subscribed": result.wasSubscribed,
                "episodes_deleted": result.episodesDeleted,
            ]
            if let title = result.title { payload["title"] = title }
            payload["message"] = result.wasSubscribed
                ? "Unsubscribed and deleted \(result.episodesDeleted) episode\(result.episodesDeleted == 1 ? "" : "s")."
                : "Deleted \(result.episodesDeleted) episode\(result.episodesDeleted == 1 ? "" : "s") from a non-subscribed podcast."
            return toolSuccess(payload)
        } catch {
            return toolError("delete_podcast failed: \(error.localizedDescription)")
        }
    }

    // MARK: - subscribe_podcast

    static func subscribePodcastTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let feedURL = (args["feed_url"] as? String)?.trimmed, !feedURL.isEmpty else {
            return toolError("Missing or empty 'feed_url'")
        }
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
            return toolSuccess(payload)
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
            return toolSuccess(payload)
        } catch {
            return toolError("download_and_transcribe failed: \(error.localizedDescription)")
        }
    }

    // MARK: - Serialization helpers

    private static func serializeDirectoryHit(_ hit: PodcastDirectoryHit) -> [String: Any] {
        var row: [String: Any] = ["podcast_title": hit.podcastTitle]
        if let id = hit.collectionID { row["collection_id"] = id }
        if let author = hit.author { row["author"] = author }
        if let feedURL = hit.feedURL { row["feed_url"] = feedURL }
        if let art = hit.artworkURL { row["artwork_url"] = art }
        if let title = hit.episodeTitle { row["episode_title"] = title }
        if let audioURL = hit.episodeAudioURL { row["audio_url"] = audioURL }
        if let guid = hit.episodeGUID { row["episode_guid"] = guid }
        if let publishedAt = hit.episodePublishedAt {
            row["published_at"] = iso8601Basic.string(from: publishedAt)
        }
        if let dur = hit.episodeDurationSeconds { row["duration_seconds"] = dur }
        if let desc = hit.episodeDescription { row["description"] = desc }
        return row
    }

    private static func clampedDirectoryLimit(_ raw: Any?) -> Int {
        let asInt: Int
        if let i = raw as? Int { asInt = i }
        else if let d = raw as? Double { asInt = Int(d) }
        else if let n = raw as? NSNumber { asInt = n.intValue }
        else { return directorySearchDefaultLimit }
        return Swift.max(1, Swift.min(directorySearchMaxLimit, asInt))
    }
}
