import Foundation

// MARK: - Podcast tool surface (lane 10)
//
// `dispatchPodcast` takes an explicit `deps: PodcastAgentToolDeps` parameter
// rather than reaching into a singleton. The orchestrator wires deps in at
// merge time. Every handler returns a JSON-serialisable string built via
// `toolSuccess` / `toolError` from the base `AgentTools` enum, so the agent
// loop's `role:tool` message round-trip is unchanged.

extension AgentTools {

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
        static let searchEpisodes       = "search_episodes"
        static let queryWiki            = "query_wiki"
        static let createWikiPage       = "create_wiki_page"
        static let listWikiPages        = "list_wiki_pages"
        static let deleteWikiPage       = "delete_wiki_page"
        static let queryTranscripts     = "query_transcripts"
        static let generateBriefing     = "generate_briefing"
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
                searchEpisodes, queryWiki, createWikiPage, listWikiPages, deleteWikiPage,
                queryTranscripts,
                generateBriefing, perplexitySearch, summarizeEpisode,
                findSimilarEpisodes, markEpisodePlayed, markEpisodeUnplayed,
                downloadEpisode, requestTranscription, refreshFeed,
                endConversation, sendFriendMessage,
                listSubscriptions, listPodcasts, listCategories, changePodcastCategory,
                listEpisodes, listInProgress, listRecentUnplayed,
                createClip, downloadAndTranscribe,
                generateTTSEpisode, configureAgentVoice, listAvailableVoices,
                searchPodcastDirectory, subscribePodcast, deletePodcast,
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
    /// Default limit for wiki page queries.
    static let podcastWikiDefaultLimit = 5
    /// Default `k` for find_similar_episodes.
    static let findSimilarDefaultK = 5
    /// Hard cap on briefing length minutes — protects the briefing composer
    /// from a runaway prompt.
    static let briefingMaxLengthMinutes = 30
    static let briefingMinLengthMinutes = 3

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
        case PodcastNames.searchEpisodes:
            return await searchEpisodesTool(args: args, deps: deps)
        case PodcastNames.queryWiki:
            return await queryWikiTool(args: args, deps: deps)
        case PodcastNames.createWikiPage:
            return await createWikiPageTool(args: args, deps: deps)
        case PodcastNames.listWikiPages:
            return await listWikiPagesTool(args: args, deps: deps)
        case PodcastNames.deleteWikiPage:
            return await deleteWikiPageTool(args: args, deps: deps)
        case PodcastNames.queryTranscripts:
            return await queryTranscriptsTool(args: args, deps: deps)
        case PodcastNames.generateBriefing:
            return await generateBriefingTool(args: args, deps: deps)
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
        guard let query = (args["query"] as? String)?.trimmed, !query.isEmpty else {
            return toolError("Missing or empty 'query'")
        }
        let scope = (args["scope"] as? String)?.trimmed.nilIfEmpty
        let limit = clampedLimit(args["limit"], default: podcastSearchDefaultLimit, max: podcastSearchMaxLimit)
        do {
            let hits = try await deps.rag.searchEpisodes(query: query, scope: scope, limit: limit)
            let rows = hits.map(serializeEpisodeHit)
            return toolSuccess([
                "query": query,
                "total_found": rows.count,
                "results": rows,
            ])
        } catch {
            return toolError("search_episodes failed: \(error.localizedDescription)")
        }
    }

    // MARK: - query_transcripts

    private static func queryTranscriptsTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let query = (args["query"] as? String)?.trimmed, !query.isEmpty else {
            return toolError("Missing or empty 'query'")
        }
        let scope = (args["scope"] as? String)?.trimmed.nilIfEmpty
        let limit = clampedLimit(args["limit"], default: podcastTranscriptDefaultLimit, max: podcastSearchMaxLimit)
        do {
            let hits = try await deps.rag.queryTranscripts(query: query, scope: scope, limit: limit)
            let rows = hits.map { hit -> [String: Any] in
                var row: [String: Any] = [
                    "episode_id": hit.episodeID,
                    "start_seconds": hit.startSeconds,
                    "end_seconds": hit.endSeconds,
                    "text": hit.text,
                ]
                if let speaker = hit.speaker { row["speaker"] = speaker }
                if let s = hit.score { row["score"] = s }
                return row
            }
            return toolSuccess([
                "query": query,
                "total_found": rows.count,
                "results": rows,
            ])
        } catch {
            return toolError("query_transcripts failed: \(error.localizedDescription)")
        }
    }

    // MARK: - generate_briefing

    private static func generateBriefingTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let scope = (args["scope"] as? String)?.trimmed, !scope.isEmpty else {
            return toolError("Missing or empty 'scope'")
        }
        guard let lengthRaw = args["length"] as? Int else {
            return toolError("Missing 'length' (target minutes)")
        }
        let length = max(briefingMinLengthMinutes, min(briefingMaxLengthMinutes, lengthRaw))
        let style = (args["style"] as? String)?.trimmed.nilIfEmpty
        do {
            let result = try await deps.briefing.composeBriefing(
                scope: scope,
                lengthMinutes: length,
                style: style
            )
            var payload: [String: Any] = [
                "briefing_id": result.briefingID,
                "title": result.title,
                "estimated_seconds": result.estimatedSeconds,
                "episode_ids": result.episodeIDs,
                "scope": scope,
                "length_minutes": length,
            ]
            if let preview = result.scriptPreview { payload["script_preview"] = preview }
            if let style = style { payload["style"] = style }
            return toolSuccess(payload)
        } catch {
            return toolError("generate_briefing failed: \(error.localizedDescription)")
        }
    }

    // MARK: - perplexity_search

    private static func perplexitySearchTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let query = (args["query"] as? String)?.trimmed, !query.isEmpty else {
            return toolError("Missing or empty 'query'")
        }
        do {
            let result = try await deps.perplexity.search(query: query)
            let sources = result.sources.map { src -> [String: Any] in
                ["title": src.title, "url": src.url]
            }
            return toolSuccess([
                "query": query,
                "answer": result.answer,
                "sources": sources,
            ])
        } catch {
            return toolError("perplexity_search failed: \(error.localizedDescription)")
        }
    }

    // MARK: - summarize_episode

    private static func summarizeEpisodeTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let episodeID = (args["episode_id"] as? String)?.trimmed, !episodeID.isEmpty else {
            return toolError("Missing or empty 'episode_id'")
        }
        let exists = await deps.fetcher.episodeExists(episodeID: episodeID)
        guard exists else {
            return toolError("Episode not found: \(episodeID)")
        }
        let length = (args["length"] as? String)?.trimmed.nilIfEmpty
        do {
            let summary = try await deps.summarizer.summarizeEpisode(
                episodeID: episodeID,
                length: length
            )
            var payload: [String: Any] = [
                "episode_id": summary.episodeID,
                "summary": summary.summary,
                "summary_source": summary.source.rawValue,
            ]
            if !summary.bulletPoints.isEmpty {
                payload["bullets"] = summary.bulletPoints
            }
            if let length = length { payload["length"] = length }
            return toolSuccess(payload)
        } catch {
            return toolError("summarize_episode failed: \(error.localizedDescription)")
        }
    }

    // MARK: - find_similar_episodes

    private static func findSimilarEpisodesTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let seed = (args["seed_episode_id"] as? String)?.trimmed, !seed.isEmpty else {
            return toolError("Missing or empty 'seed_episode_id'")
        }
        let exists = await deps.fetcher.episodeExists(episodeID: seed)
        guard exists else {
            return toolError("Seed episode not found: \(seed)")
        }
        let k = clampedLimit(args["k"], default: findSimilarDefaultK, max: 20)
        do {
            let hits = try await deps.rag.findSimilarEpisodes(seedEpisodeID: seed, k: k)
            let rows = hits.map(serializeEpisodeHit)
            return toolSuccess([
                "seed_episode_id": seed,
                "k": k,
                "total_found": rows.count,
                "results": rows,
            ])
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

    /// Build the JSON-serializable row for an `EpisodeHit`. Shared by
    /// `search_episodes` and `find_similar_episodes`.
    private static func serializeEpisodeHit(_ hit: EpisodeHit) -> [String: Any] {
        var row: [String: Any] = [
            "episode_id": hit.episodeID,
            "podcast_id": hit.podcastID,
            "title": hit.title,
            "podcast_title": hit.podcastTitle,
        ]
        if let publishedAt = hit.publishedAt {
            row["published_at"] = iso8601Basic.string(from: publishedAt)
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
}

// MARK: - String helpers

extension String {
    /// Module-internal so sibling `AgentTools+*.swift` files share one definition.
    var nilIfEmpty: String? { isEmpty ? nil : self }
}
