import Foundation

// MARK: - Podcast action tools

extension AgentTools {

    // MARK: - Playback controls

    static func pausePlaybackTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard await deps.playback.pausePlayback() else {
            return toolError("Playback is unavailable.")
        }
        return toolSuccess(["state": "paused"])
    }

    static func setPlaybackRateTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let requested = podcastActionNumericArg(args["rate"]) else {
            return toolError("Missing or invalid 'rate'")
        }
        guard requested > 0 else {
            return toolError("'rate' must be greater than 0")
        }
        guard let applied = await deps.playback.setPlaybackRate(requested) else {
            return toolError("Playback is unavailable.")
        }
        return toolSuccess([
            "requested_rate": requested,
            "rate": applied,
        ])
    }

    static func setSleepTimerTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let mode = (args["mode"] as? String)?.trimmed.nilIfEmpty else {
            return toolError("Missing or empty 'mode'")
        }
        let normalized = mode.lowercased()
        guard ["off", "minutes", "end_of_episode"].contains(normalized) else {
            return toolError("'mode' must be one of: off, minutes, end_of_episode")
        }
        let minutes: Int?
        if normalized == "minutes" {
            guard let rawMinutes = podcastActionIntArg(args["minutes"]), rawMinutes > 0 else {
                return toolError("'minutes' is required when mode is 'minutes'")
            }
            minutes = min(180, rawMinutes)
        } else {
            minutes = nil
        }
        guard let label = await deps.playback.setSleepTimer(mode: normalized, minutes: minutes) else {
            return toolError("Playback is unavailable.")
        }
        var payload: [String: Any] = [
            "mode": normalized,
            "label": label,
        ]
        if let minutes { payload["minutes"] = minutes }
        return toolSuccess(payload)
    }

    // MARK: - Episode state

    static func markEpisodePlayedTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        await episodeMutationTool(
            args: args,
            deps: deps,
            action: "mark_episode_played",
            mutate: deps.library.markEpisodePlayed
        )
    }

    static func markEpisodeUnplayedTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        await episodeMutationTool(
            args: args,
            deps: deps,
            action: "mark_episode_unplayed",
            mutate: deps.library.markEpisodeUnplayed
        )
    }

    static func downloadEpisodeTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        await episodeMutationTool(
            args: args,
            deps: deps,
            action: "download_episode",
            mutate: deps.library.downloadEpisode
        )
    }

    private static func episodeMutationTool(
        args: [String: Any],
        deps: PodcastAgentToolDeps,
        action: String,
        mutate: @escaping (EpisodeID) async throws -> EpisodeMutationResult
    ) async -> String {
        guard let episodeID = (args["episode_id"] as? String)?.trimmed, !episodeID.isEmpty else {
            return toolError("Missing or empty 'episode_id'")
        }
        let exists = await deps.fetcher.episodeExists(episodeID: episodeID)
        guard exists else {
            return toolError("Episode not found: \(episodeID)")
        }
        do {
            return toolSuccess(serializeEpisodeMutation(try await mutate(episodeID)))
        } catch {
            return toolError("\(action) failed: \(error.localizedDescription)")
        }
    }

    // MARK: - Transcript + feed

    static func requestTranscriptionTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let episodeID = (args["episode_id"] as? String)?.trimmed, !episodeID.isEmpty else {
            return toolError("Missing or empty 'episode_id'")
        }
        let exists = await deps.fetcher.episodeExists(episodeID: episodeID)
        guard exists else {
            return toolError("Episode not found: \(episodeID)")
        }
        do {
            let result = try await deps.library.requestTranscription(episodeID: episodeID)
            var payload: [String: Any] = [
                "episode_id": result.episodeID,
                "status": result.status,
            ]
            if let source = result.source { payload["source"] = source }
            if let message = result.message { payload["message"] = message }
            return toolSuccess(payload)
        } catch {
            return toolError("request_transcription failed: \(error.localizedDescription)")
        }
    }

    static func downloadAndTranscribeTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        let episodeIDRaw = (args["episode_id"] as? String)?.trimmed.nilIfEmpty
        let audioURLRaw  = (args["audio_url"] as? String)?.trimmed.nilIfEmpty
        let feedURLRaw   = (args["feed_url"] as? String)?.trimmed.nilIfEmpty

        // External path: no episode_id — route through auto-subscribe.
        if episodeIDRaw == nil, let audioURL = audioURLRaw {
            guard let feedURL = feedURLRaw else {
                return toolError("'feed_url' is required when 'episode_id' is not provided. " +
                    "Use subscribe_podcast or search_podcast_directory to get the feed URL first.")
            }
            return await downloadAndTranscribeExternalTool(
                feedURLString: feedURL,
                audioURLString: audioURL,
                deps: deps
            )
        }

        guard let episodeID = episodeIDRaw else {
            return toolError("Provide 'episode_id' (for subscribed episodes) or " +
                "'audio_url' + 'feed_url' (for external episodes)")
        }
        let exists = await deps.fetcher.episodeExists(episodeID: episodeID)
        guard exists else {
            return toolError("Episode not found: \(episodeID)")
        }
        do {
            let result = try await deps.library.downloadAndTranscribe(episodeID: episodeID)
            var payload: [String: Any] = [
                "episode_id": result.episodeID,
                "status": result.status,
            ]
            if let source = result.source { payload["source"] = source }
            if let message = result.message { payload["message"] = message }
            return toolSuccess(payload)
        } catch {
            return toolError("download_and_transcribe failed: \(error.localizedDescription)")
        }
    }

    static func refreshFeedTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let podcastID = (args["podcast_id"] as? String)?.trimmed, !podcastID.isEmpty else {
            return toolError("Missing or empty 'podcast_id'")
        }
        do {
            let result = try await deps.library.refreshFeed(podcastID: podcastID)
            var payload: [String: Any] = [
                "podcast_id": result.podcastID,
                "title": result.title,
                "episode_count": result.episodeCount,
                "new_episode_count": result.newEpisodeCount,
            ]
            if let refreshedAt = result.refreshedAt {
                payload["refreshed_at"] = iso8601Basic.string(from: refreshedAt)
            }
            return toolSuccess(payload)
        } catch {
            return toolError("refresh_feed failed: \(error.localizedDescription)")
        }
    }

    // MARK: - Clipping

    static func createClipTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let episodeID = (args["episode_id"] as? String)?.trimmed, !episodeID.isEmpty else {
            return toolError("Missing or empty 'episode_id'")
        }
        guard let startSeconds = podcastActionNumericArg(args["start_seconds"]) else {
            return toolError("Missing or invalid 'start_seconds'")
        }
        guard let endSeconds = podcastActionNumericArg(args["end_seconds"]) else {
            return toolError("Missing or invalid 'end_seconds'")
        }
        guard startSeconds >= 0 else {
            return toolError("'start_seconds' must be >= 0")
        }
        guard endSeconds > startSeconds else {
            return toolError("'end_seconds' must be greater than 'start_seconds'")
        }
        let exists = await deps.fetcher.episodeExists(episodeID: episodeID)
        guard exists else {
            return toolError("Episode not found: \(episodeID)")
        }
        let caption = (args["caption"] as? String)?.trimmed.nilIfEmpty
        let transcriptText = (args["transcript_text"] as? String)?.trimmed.nilIfEmpty
        do {
            let result = try await deps.library.createClip(
                episodeID: episodeID,
                startSeconds: startSeconds,
                endSeconds: endSeconds,
                caption: caption,
                transcriptText: transcriptText
            )
            var payload: [String: Any] = [
                "clip_id": result.clipID,
                "episode_id": result.episodeID,
                "episode_title": result.episodeTitle,
                "start_seconds": result.startSeconds,
                "end_seconds": result.endSeconds,
                "duration_seconds": result.endSeconds - result.startSeconds,
            ]
            if !result.transcriptText.isEmpty { payload["transcript_text"] = result.transcriptText }
            if let caption = result.caption { payload["caption"] = caption }
            if let podcastID = result.podcastID { payload["podcast_id"] = podcastID }
            return toolSuccess(payload)
        } catch {
            return toolError("create_clip failed: \(error.localizedDescription)")
        }
    }

    // MARK: - play_episode

    /// Unified playback verb. Plays a single episode — identified either by
    /// `episode_id` (library) or by `audio_url` + `title` (one-off URL, no
    /// subscription required) — at an optional `start_seconds` / `end_seconds`
    /// window, routed by `queue_position` (defaults to `.now`).
    static func playEpisodeTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        let episodeID = (args["episode_id"] as? String)?.trimmed.nilIfEmpty
        let audioURLString = (args["audio_url"] as? String)?.trimmed.nilIfEmpty

        if episodeID != nil, audioURLString != nil {
            return toolError("Pass either 'episode_id' OR 'audio_url' — not both.")
        }
        if episodeID == nil, audioURLString == nil {
            return toolError("Missing identifier: provide 'episode_id' (library) or 'audio_url' + 'title' (external).")
        }

        let startSeconds = podcastActionNumericArg(args["start_seconds"])
        if let s = startSeconds, s < 0 {
            return toolError("'start_seconds' must be >= 0")
        }
        let endSeconds = podcastActionNumericArg(args["end_seconds"])
        if let e = endSeconds, let s = startSeconds, e <= s {
            return toolError("'end_seconds' must be greater than 'start_seconds'")
        }
        if let e = endSeconds, startSeconds == nil, e <= 0 {
            return toolError("'end_seconds' must be > 0 when 'start_seconds' is omitted")
        }
        let positionRaw = (args["queue_position"] as? String)?.trimmed.lowercased() ?? QueuePosition.now.rawValue
        guard let position = QueuePosition(rawValue: positionRaw) else {
            return toolError("'queue_position' must be one of: now, next, end")
        }

        if let episodeID {
            return await playLibraryEpisode(
                episodeID: episodeID,
                startSeconds: startSeconds,
                endSeconds: endSeconds,
                position: position,
                deps: deps
            )
        }
        // audioURLString is non-nil here (validated above).
        return await playExternalAudioURL(
            audioURLString: audioURLString ?? "",
            args: args,
            startSeconds: startSeconds,
            endSeconds: endSeconds,
            position: position,
            deps: deps
        )
    }

    private static func playLibraryEpisode(
        episodeID: String,
        startSeconds: Double?,
        endSeconds: Double?,
        position: QueuePosition,
        deps: PodcastAgentToolDeps
    ) async -> String {
        let exists = await deps.fetcher.episodeExists(episodeID: episodeID)
        guard exists else {
            return toolError("Episode not found: \(episodeID)")
        }
        guard let result = await deps.playback.playEpisode(
            episodeID: episodeID,
            startSeconds: startSeconds,
            endSeconds: endSeconds,
            queuePosition: position
        ) else {
            return toolError("play_episode failed: playback host unavailable")
        }
        return toolSuccess(serializePlayEpisodeResult(result, startSeconds: startSeconds, endSeconds: endSeconds))
    }

    private static func playExternalAudioURL(
        audioURLString: String,
        args: [String: Any],
        startSeconds: Double?,
        endSeconds: Double?,
        position: QueuePosition,
        deps: PodcastAgentToolDeps
    ) async -> String {
        guard let audioURL = URL(string: audioURLString) else {
            return toolError("Invalid 'audio_url': \(audioURLString)")
        }
        guard let title = (args["title"] as? String)?.trimmed, !title.isEmpty else {
            return toolError("Missing or empty 'title' (required with 'audio_url').")
        }
        let feedURLString = (args["feed_url"] as? String)?.trimmed.nilIfEmpty
        let durationSeconds = podcastActionNumericArg(args["duration_seconds"])
        guard let result = await deps.playback.playExternalEpisode(
            audioURL: audioURL,
            title: title,
            feedURLString: feedURLString,
            durationSeconds: durationSeconds,
            startSeconds: startSeconds,
            endSeconds: endSeconds,
            queuePosition: position
        ) else {
            return toolError("play_episode failed: playback host unavailable")
        }
        var payload = serializePlayEpisodeResult(result, startSeconds: startSeconds, endSeconds: endSeconds)
        payload["audio_url"] = audioURLString
        payload["title"] = title
        if let feedURLString { payload["feed_url"] = feedURLString }
        return toolSuccess(payload)
    }

    /// Shared payload shape for both `play_episode` branches (library and
    /// external) so the LLM sees a consistent success envelope.
    static func serializePlayEpisodeResult(
        _ result: PlayEpisodeResult,
        startSeconds: Double?,
        endSeconds: Double?
    ) -> [String: Any] {
        var payload: [String: Any] = [
            "episode_id": result.episodeID,
            "queue_position": result.queuePosition.rawValue,
            "started_playing": result.startedPlaying,
        ]
        if let title = result.episodeTitle { payload["episode_title"] = title }
        if let podcast = result.podcastTitle { payload["podcast_title"] = podcast }
        if let dur = result.durationSeconds { payload["duration_seconds"] = dur }
        if let s = startSeconds { payload["start_seconds"] = s }
        if let e = endSeconds { payload["end_seconds"] = e }
        switch result.queuePosition {
        case .now:
            payload["status"] = "playing"
            payload["message"] = "Playing now."
        case .next:
            payload["status"] = "queued"
            payload["message"] = "Added to the front of Up Next."
        case .end:
            payload["status"] = "queued"
            payload["message"] = "Added to the end of Up Next."
        }
        return payload
    }

    // MARK: - Helpers

    private static func serializeEpisodeMutation(_ result: EpisodeMutationResult) -> [String: Any] {
        var payload: [String: Any] = [
            "episode_id": result.episodeID,
            "episode_title": result.episodeTitle,
            "state": result.state,
        ]
        if let podcastID = result.podcastID { payload["podcast_id"] = podcastID }
        if let podcastTitle = result.podcastTitle { payload["podcast_title"] = podcastTitle }
        return payload
    }

    static func podcastActionNumericArg(_ raw: Any?) -> Double? {
        if let d = raw as? Double { return d }
        if let i = raw as? Int { return Double(i) }
        if let n = raw as? NSNumber { return n.doubleValue }
        return nil
    }

    static func podcastActionIntArg(_ raw: Any?) -> Int? {
        if let i = raw as? Int { return i }
        if let d = raw as? Double { return Int(d) }
        if let n = raw as? NSNumber { return n.intValue }
        return nil
    }
}

// `nilIfEmpty` lives at internal scope on `String` in
// `AgentTools+Podcast.swift` so all three `AgentTools+*.swift` files
// share one definition.
