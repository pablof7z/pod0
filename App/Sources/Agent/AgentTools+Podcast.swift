import Foundation

// MARK: - Podcast tool surface (lane 10)
//
// `dispatchPodcast` takes an explicit `deps: PodcastAgentToolDeps` parameter
// rather than reaching into a singleton. The orchestrator wires deps in at
// merge time. Every handler returns a JSON-serialisable string built via
// `toolSuccess` / `toolError` from the base `AgentTools` enum, so the agent
// loop's `role:tool` message round-trip is unchanged.

extension AgentTools {
    private struct SearchPlan: Decodable {
        let error: String?
        let query: String?
        let scope: String?
        let limit: Int?
        let retrievalLimit: Int?

        enum CodingKeys: String, CodingKey {
            case error
            case query
            case scope
            case limit
            case retrievalLimit = "retrieval_limit"
        }
    }

    private struct SimilarSearchPlan: Decodable {
        let error: String?
        let seedEpisodeID: String?
        let k: Int?

        enum CodingKeys: String, CodingKey {
            case error, k
            case seedEpisodeID = "seed_episode_id"
        }
    }

    private struct SummaryPlan: Decodable {
        let error: String?
        let episodeID: String?

        enum CodingKeys: String, CodingKey {
            case error
            case episodeID = "episode_id"
        }
    }

    // MARK: - Tool name constants

    /// Canonical string identifiers for the podcast-domain tools.
    /// Mirrors `AgentTools.Names`; kept as a separate nested enum so this lane
    /// owns its own surface without modifying the read-only base file.
    enum PodcastNames {
        /// Unified playback verb. Plays a single episode (optionally bounded
        /// by start/end seconds) and routes via `queue_position` so the same
        /// tool covers play-now, play-next, and append-to-end. Replaces the
        /// pre-split `play_episode_at` + `queue_episode_segments` pair.
        static let playEpisode          = "play_episode"
        static let pausePlayback        = "pause_playback"
        static let setPlaybackRate      = "set_playback_rate"
        static let setSleepTimer        = "set_sleep_timer"
        static let getNowPlaying        = "get_now_playing"
        static let seekTo               = "seek_to"
        static let skipForward          = "skip_forward"
        static let skipBackward         = "skip_backward"
        static let searchEpisodes       = "search_episodes"
        static let queryTranscripts     = "query_transcripts"
        static let perplexitySearch     = "perplexity_search"
        static let summarizeEpisode     = "summarize_episode"
        static let findSimilarEpisodes  = "find_similar_episodes"
        static let markEpisodePlayed    = "mark_episode_played"
        static let markEpisodeUnplayed  = "mark_episode_unplayed"
        static let downloadEpisode      = "download_episode"
        static let requestTranscription = "request_transcription"
        static let refreshFeed          = "refresh_feed"
        static let endConversation      = "end_conversation"
        static let sendFriendMessage    = "send_friend_message"
        static let listSubscriptions    = "list_subscriptions"
        static let listPodcasts         = "list_podcasts"
        static let listCategories       = "list_categories"
        static let changePodcastCategory = "change_podcast_category"
        static let listEpisodes         = "list_episodes"
        static let listInProgress       = "list_in_progress"
        static let listRecentUnplayed   = "list_recent_unplayed"
        static let createClip             = "create_clip"
        static let downloadAndTranscribe  = "download_and_transcribe"
        static let generateTTSEpisode     = "generate_tts_episode"
        static let configureAgentVoice    = "configure_agent_voice"
        /// Skill-gated: only callable when the `podcast_generation` skill
        /// is enabled. See `PodcastGenerationSkill`.
        static let listAvailableVoices    = "list_available_voices"

        // External-podcast tools
        static let searchPodcastDirectory = "search_podcast_directory"
        static let subscribePodcast       = "subscribe_podcast"
        static let unfollowPodcast        = "unfollow_podcast"
        static let deletePodcast          = "delete_podcast"

        // Skill-gated: requires the `youtube_ingestion` skill.
        static let ingestYouTubeVideo     = "ingest_youtube_video"
        static let searchYouTube          = "search_youtube"

        // Agent-owned podcast management
        static let createPodcast          = "create_podcast"
        static let updatePodcast          = "update_podcast"
        static let deleteMyPodcast        = "delete_my_podcast"
        static let listMyPodcasts         = "list_my_podcasts"
        static let generatePodcastArtwork = "generate_podcast_artwork"
        static let publishEpisode         = "publish_episode"

        /// Every podcast tool name, for orchestrator convenience when wiring
        /// the main `AgentTools.dispatch` switch. Skill-gated names are
        /// included here so `dispatch` can route them; whether they are
        /// callable from a given session is gated separately by the
        /// `enabledSkills` check in `dispatchPodcast`.
        static var all: [String] {
            [
                playEpisode, pausePlayback, setPlaybackRate, setSleepTimer,
                getNowPlaying, seekTo, skipForward, skipBackward,
                searchEpisodes, queryTranscripts,
                perplexitySearch, summarizeEpisode,
                findSimilarEpisodes, markEpisodePlayed, markEpisodeUnplayed,
                downloadEpisode, requestTranscription, refreshFeed,
                endConversation, sendFriendMessage,
                listSubscriptions, listPodcasts, listCategories, changePodcastCategory,
                listEpisodes, listInProgress, listRecentUnplayed,
                createClip, downloadAndTranscribe,
                generateTTSEpisode, configureAgentVoice, listAvailableVoices,
                searchPodcastDirectory, subscribePodcast, unfollowPodcast, deletePodcast,
                ingestYouTubeVideo, searchYouTube,
                createPodcast, updatePodcast, deleteMyPodcast, listMyPodcasts, generatePodcastArtwork,
                publishEpisode,
            ]
        }
    }

    // MARK: - Result-shape limits

    /// Maximum hits returned by `search_episodes` / `find_similar_episodes` /
    /// `query_transcripts` regardless of what the model requests.
    static let podcastSearchMaxLimit = 25
    /// Default limit when the model omits `limit`.
    static let podcastSearchDefaultLimit = 10
    /// Default limit for transcript-chunk queries (typically smaller payload
    /// per result, but each chunk is verbose).
    static let podcastTranscriptDefaultLimit = 8
    /// Default `k` for find_similar_episodes.
    static let findSimilarDefaultK = 5

    // MARK: - Dispatcher

    /// Routes a podcast-domain tool call by name. Throws no errors — every
    /// failure path becomes a JSON `error` envelope so the agent loop can
    /// continue with a `role:tool` message.
    static func dispatchPodcast(
        name: String,
        argsJSON: String,
        deps: PodcastAgentToolDeps,
        enabledSkills: Set<String> = []
    ) async -> String {
        let args: [String: Any]
        do {
            args = try JSONSerialization.jsonObject(with: Data(argsJSON.utf8)) as? [String: Any] ?? [:]
        } catch {
            logger.error("AgentTools+Podcast: failed to parse argsJSON for tool '\(name, privacy: .public)': \(error.localizedDescription, privacy: .public)")
            return toolError("Invalid JSON arguments")
        }
        return await dispatchPodcast(name: name, args: args, deps: deps, enabledSkills: enabledSkills)
    }

    /// Args-already-parsed variant. Exposed `internal` so tests can call it
    /// without round-tripping through `JSONSerialization`.
    static func dispatchPodcast(
        name: String,
        args: [String: Any],
        deps: PodcastAgentToolDeps,
        enabledSkills: Set<String> = []
    ) async -> String {
        // Defensive skill gate. The LLM should never see the schema for a
        // gated tool unless its owning skill is enabled, but if it somehow
        // calls one anyway we surface a clear error instead of running the
        // handler unauthenticated.
        if let owningSkill = AgentSkillRegistry.owningSkillID(forTool: name),
           !enabledSkills.contains(owningSkill) {
            return toolError("Tool '\(name)' requires the '\(owningSkill)' skill — call use_skill(skill_id: \"\(owningSkill)\") first.")
        }
        switch name {
        case PodcastNames.playEpisode:
            return await playEpisodeTool(args: args, deps: deps)
        case PodcastNames.pausePlayback:
            return await pausePlaybackTool(args: args, deps: deps)
        case PodcastNames.setPlaybackRate:
            return await setPlaybackRateTool(args: args, deps: deps)
        case PodcastNames.setSleepTimer:
            return await setSleepTimerTool(args: args, deps: deps)
        case PodcastNames.getNowPlaying:
            return await getNowPlayingTool(args: args, deps: deps)
        case PodcastNames.seekTo:
            return await seekToTool(args: args, deps: deps)
        case PodcastNames.skipForward:
            return await skipForwardTool(args: args, deps: deps)
        case PodcastNames.skipBackward:
            return await skipBackwardTool(args: args, deps: deps)
        case PodcastNames.searchEpisodes:
            return await searchEpisodesTool(args: args, deps: deps)
        case PodcastNames.queryTranscripts:
            return await queryTranscriptsTool(args: args, deps: deps)
        case PodcastNames.perplexitySearch:
            return await perplexitySearchTool(args: args, deps: deps)
        case PodcastNames.summarizeEpisode:
            return await summarizeEpisodeTool(args: args, deps: deps)
        case PodcastNames.findSimilarEpisodes:
            return await findSimilarEpisodesTool(args: args, deps: deps)
        case PodcastNames.markEpisodePlayed:
            return await markEpisodePlayedTool(args: args, deps: deps)
        case PodcastNames.markEpisodeUnplayed:
            return await markEpisodeUnplayedTool(args: args, deps: deps)
        case PodcastNames.downloadEpisode:
            return await downloadEpisodeTool(args: args, deps: deps)
        case PodcastNames.requestTranscription:
            return await requestTranscriptionTool(args: args, deps: deps)
        case PodcastNames.refreshFeed:
            return await refreshFeedTool(args: args, deps: deps)
        case PodcastNames.endConversation:
            return await endConversationTool(args: args, deps: deps)
        case PodcastNames.sendFriendMessage:
            return await sendFriendMessageTool(args: args, deps: deps)
        case PodcastNames.listSubscriptions:
            return await listSubscriptionsTool(args: args, deps: deps)
        case PodcastNames.listPodcasts:
            return await listPodcastsTool(args: args, deps: deps)
        case PodcastNames.listCategories:
            return await listCategoriesTool(args: args, deps: deps)
        case PodcastNames.changePodcastCategory:
            return await changePodcastCategoryTool(args: args, deps: deps)
        case PodcastNames.listEpisodes:
            return await listEpisodesTool(args: args, deps: deps)
        case PodcastNames.listInProgress:
            return await listInProgressTool(args: args, deps: deps)
        case PodcastNames.listRecentUnplayed:
            return await listRecentUnplayedTool(args: args, deps: deps)
        case PodcastNames.createClip:
            return await createClipTool(args: args, deps: deps)
        case PodcastNames.downloadAndTranscribe:
            return await downloadAndTranscribeTool(args: args, deps: deps)
        case PodcastNames.generateTTSEpisode:
            return await generateTTSEpisodeTool(args: args, deps: deps)
        case PodcastNames.configureAgentVoice:
            return await configureAgentVoiceTool(args: args, deps: deps)
        case PodcastNames.listAvailableVoices:
            return await listAvailableVoicesTool(args: args)
        case PodcastNames.searchPodcastDirectory:
            return await searchPodcastDirectoryTool(args: args, deps: deps)
        case PodcastNames.subscribePodcast:
            return await subscribePodcastTool(args: args, deps: deps)
        case PodcastNames.unfollowPodcast:
            return await unfollowPodcastTool(args: args, deps: deps)
        case PodcastNames.deletePodcast:
            return await deletePodcastTool(args: args, deps: deps)
        case PodcastNames.ingestYouTubeVideo:
            return await ingestYouTubeVideoTool(args: args, deps: deps)
        case PodcastNames.searchYouTube:
            return await searchYouTubeTool(args: args, deps: deps)
        case PodcastNames.createPodcast:
            return await createPodcastTool(args: args, deps: deps)
        case PodcastNames.updatePodcast:
            return await updatePodcastTool(args: args, deps: deps)
        case PodcastNames.deleteMyPodcast:
            return await deleteMyPodcastTool(args: args, deps: deps)
        case PodcastNames.listMyPodcasts:
            return await listMyPodcastsTool(args: args, deps: deps)
        case PodcastNames.generatePodcastArtwork:
            return await generatePodcastArtworkTool(args: args, deps: deps)
        case PodcastNames.publishEpisode:
            return await publishEpisodeTool(args: args, deps: deps)
        default:
            return toolError("Unknown podcast tool: \(name)")
        }
    }

    // Inventory tools live in `AgentTools+PodcastInventory.swift`.
    // `play_episode` handler lives in `AgentTools+PodcastActions.swift`.

    // MARK: - search_episodes

    private static func searchEpisodesTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let plan = await searchPlan(op: "search_plan", args: args) else {
            return toolError("search_episodes planning is unavailable")
        }
        if let error = plan.error { return toolError(error) }
        guard let query = plan.query, let limit = plan.limit else {
            return toolError("search_episodes plan was incomplete")
        }
        do {
            let hits = try await deps.rag.searchEpisodes(
                query: query,
                scope: plan.scope,
                limit: limit,
                retrievalLimit: plan.retrievalLimit ?? limit
            )
            return await searchTool(
                op: "episode_results",
                payload: [
                "query": query,
                    "results": hits.map(rawEpisodeHit),
                ]
            ) ?? toolError("search_episodes result shaping is unavailable")
        } catch {
            return toolError("search_episodes failed: \(error.localizedDescription)")
        }
    }

    // MARK: - query_transcripts

    private static func queryTranscriptsTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let plan = await searchPlan(op: "transcript_plan", args: args) else {
            return toolError("query_transcripts planning is unavailable")
        }
        if let error = plan.error { return toolError(error) }
        guard let query = plan.query, let limit = plan.limit else {
            return toolError("query_transcripts plan was incomplete")
        }
        do {
            let hits = try await deps.rag.queryTranscripts(query: query, scope: plan.scope, limit: limit)
            return await searchTool(
                op: "transcript_results",
                payload: [
                "query": query,
                    "results": hits.map(rawTranscriptHit),
                ]
            ) ?? toolError("query_transcripts result shaping is unavailable")
        } catch {
            return toolError("query_transcripts failed: \(error.localizedDescription)")
        }
    }

    // MARK: - perplexity_search

    private static func perplexitySearchTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let plan = await searchPlan(op: "perplexity_plan", args: args) else {
            return toolError("perplexity_search planning is unavailable")
        }
        if let error = plan.error { return toolError(error) }
        guard let query = plan.query else { return toolError("perplexity_search plan was incomplete") }
        do {
            let result = try await deps.perplexity.search(query: query)
            let sources = result.sources.map { src -> [String: Any] in
                ["title": src.title, "url": src.url]
            }
            return await searchTool(op: "perplexity_results", payload: [
                "query": query,
                "answer": result.answer,
                "sources": sources,
            ]) ?? toolError("perplexity_search result shaping is unavailable")
        } catch {
            return toolError("perplexity_search failed: \(error.localizedDescription)")
        }
    }

    // MARK: - summarize_episode

    private static func summarizeEpisodeTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let plan = await summaryPlan(args: args) else {
            return toolError("summarize_episode planning is unavailable")
        }
        if let error = plan.error { return toolError(error) }
        guard let episodeID = plan.episodeID else { return toolError("summarize_episode plan was incomplete") }
        // Dispatches `podcast.summarize_episode` to the Rust kernel and awaits
        // the summary on the snapshot projection. Rust owns the fallback
        // decision when the kernel summary is unavailable. The fixed "2-3 sentences" kernel prompt has no length /
        // bullet options, so the payload is a plain `{episode_id, summary}`.
        switch await deps.summarizer.summarize(episodeID: episodeID) {
        case let .summary(summary) where !summary.isEmpty:
            return await searchTool(op: "summary_result", payload: [
                "episode_id": episodeID,
                "summary": summary,
            ]) ?? toolError("summarize_episode result shaping is unavailable")
        case let .rejected(message):
            return toolError("summarize_episode rejected: \(message)")
        case .summary, .unavailable:
            return toolError("summarize_episode failed: no summary available for \(episodeID)")
        }
    }

    // MARK: - find_similar_episodes

    private static func findSimilarEpisodesTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let plan = await similarSearchPlan(args: args) else {
            return toolError("find_similar_episodes planning is unavailable")
        }
        if let error = plan.error { return toolError(error) }
        guard let seed = plan.seedEpisodeID, let k = plan.k else {
            return toolError("find_similar_episodes plan was incomplete")
        }
        guard await deps.fetcher.episodeMetadata(episodeID: seed) != nil else {
            return toolError("find_similar_episodes: seed episode '\(seed)' not found in library")
        }
        do {
            let hits = try await deps.rag.findSimilarEpisodes(seedEpisodeID: seed, k: k)
            return await searchTool(
                op: "episode_results",
                payload: [
                "seed_episode_id": seed,
                "k": k,
                    "results": hits.map(rawEpisodeHit),
                ]
            ) ?? toolError("find_similar_episodes result shaping is unavailable")
        } catch {
            return toolError("find_similar_episodes failed: \(error.localizedDescription)")
        }
    }

    // MARK: - Argument helpers

    /// Coerce JSON numerics to a `Double`. Supports `Double`, `Int`, and `NSNumber`.
    static func numericArg(_ raw: Any?) -> Double? {
        if let d = raw as? Double { return d }
        if let i = raw as? Int { return Double(i) }
        if let n = raw as? NSNumber { return n.doubleValue }
        return nil
    }

    /// Clamp a `limit`/`k` argument to `[1, max]`. Falls back to `default` when
    /// the argument is missing or non-integer.
    static func clampedLimit(_ raw: Any?, default defaultValue: Int, max: Int) -> Int {
        let asInt: Int
        if let i = raw as? Int { asInt = i }
        else if let d = raw as? Double { asInt = Int(d) }
        else if let n = raw as? NSNumber { asInt = n.intValue }
        else { return defaultValue }
        return Swift.max(1, Swift.min(max, asInt))
    }

    private static func rawEpisodeHit(_ hit: EpisodeHit) -> [String: Any] {
        var row: [String: Any] = [
            "episode_id": hit.episodeID,
            "podcast_id": hit.podcastID,
            "title": hit.title,
            "podcast_title": hit.podcastTitle,
        ]
        if let publishedAt = hit.publishedAt {
            row["published_at"] = Int(publishedAt.timeIntervalSince1970)
        }
        if let dur = hit.durationSeconds {
            row["duration_seconds"] = dur
        }
        if let snippet = hit.snippet {
            row["snippet"] = snippet
        }
        if let score = hit.score {
            row["score"] = score
        }
        return row
    }

    private static func rawTranscriptHit(_ hit: TranscriptHit) -> [String: Any] {
        var row: [String: Any] = [
            "episode_id": hit.episodeID,
            "start_seconds": hit.startSeconds,
            "end_seconds": hit.endSeconds,
            "text": hit.text,
        ]
        if let speaker = hit.speaker { row["speaker"] = speaker }
        if let score = hit.score { row["score"] = score }
        return row
    }

    private static func searchPlan(op: String, args: [String: Any]) async -> SearchPlan? {
        guard let envelope = await searchTool(op: op, payload: ["args": args]),
              let data = envelope.data(using: .utf8)
        else { return nil }
        return try? JSONDecoder().decode(SearchPlan.self, from: data)
    }

    private static func similarSearchPlan(args: [String: Any]) async -> SimilarSearchPlan? {
        guard let envelope = await searchTool(op: "similar_plan", payload: ["args": args]),
              let data = envelope.data(using: .utf8)
        else { return nil }
        return try? JSONDecoder().decode(SimilarSearchPlan.self, from: data)
    }

    private static func summaryPlan(args: [String: Any]) async -> SummaryPlan? {
        guard let envelope = await searchTool(op: "summary_plan", payload: ["args": args]),
              let data = envelope.data(using: .utf8)
        else { return nil }
        return try? JSONDecoder().decode(SummaryPlan.self, from: data)
    }

    private static func searchTool(op: String, payload: [String: Any]) async -> String? {
        let handleBits = await MainActor.run {
            KernelModel.shared?.podcastHandlePointer.map { Int(bitPattern: $0) }
        }
        guard let handleBits else { return nil }
        var request = payload
        request["op"] = op
        guard let data = try? JSONSerialization.data(withJSONObject: request),
              let json = String(data: data, encoding: .utf8)
        else { return nil }
        return await Task.detached(priority: .userInitiated) {
            guard let handle = UnsafeMutableRawPointer(bitPattern: handleBits) else {
                return nil
            }
            return json.withCString { ptr in
                guard let result = nmp_app_podcast_agent_search_tool(handle, ptr) else {
                    return nil
                }
                defer { nmp_free_string(result) }
                return String(cString: result)
            }
        }.value
    }
}

// MARK: - String helpers

extension String {
    /// Module-internal so sibling `AgentTools+*.swift` files share one definition.
    var nilIfEmpty: String? { isEmpty ? nil : self }
}
