import Foundation

// MARK: - Podcast action tools

extension AgentTools {
    private struct PlaybackRatePlan: Decodable {
        let error: String?
        let rate: Double?
    }

    private struct SleepTimerPlan: Decodable {
        let error: String?
        let mode: String?
        let minutes: Int?
    }

    private struct SeekPlan: Decodable {
        let error: String?
        let positionSeconds: Double?

        enum CodingKeys: String, CodingKey {
            case error
            case positionSeconds = "position_seconds"
        }
    }

    private struct PlayEpisodePlan: Decodable {
        let error: String?
        let source: String?
        let episodeID: String?
        let audioURL: String?
        let title: String?
        let feedURL: String?
        let durationSeconds: Double?
        let startSeconds: Double?
        let endSeconds: Double?
        let queuePosition: String?

        enum CodingKeys: String, CodingKey {
            case error, source, title
            case episodeID = "episode_id"
            case audioURL = "audio_url"
            case feedURL = "feed_url"
            case durationSeconds = "duration_seconds"
            case startSeconds = "start_seconds"
            case endSeconds = "end_seconds"
            case queuePosition = "queue_position"
        }
    }

    private struct ActionIDPlan: Decodable {
        let error: String?
        let episodeID: String?
        let podcastID: String?

        enum CodingKeys: String, CodingKey {
            case error
            case episodeID = "episode_id"
            case podcastID = "podcast_id"
        }
    }

    private struct ClipActionPlan: Decodable {
        let error: String?
        let episodeID: String?
        let startSeconds: Double?
        let endSeconds: Double?
        let caption: String?
        let transcriptText: String?

        enum CodingKeys: String, CodingKey {
            case error, caption
            case episodeID = "episode_id"
            case startSeconds = "start_seconds"
            case endSeconds = "end_seconds"
            case transcriptText = "transcript_text"
        }
    }

    private struct DownloadTranscribePlan: Decodable {
        let error: String?
        let source: String?
        let episodeID: String?
        let audioURL: String?
        let feedURL: String?

        enum CodingKeys: String, CodingKey {
            case error, source
            case episodeID = "episode_id"
            case audioURL = "audio_url"
            case feedURL = "feed_url"
        }
    }

    // MARK: - Playback controls

    static func pausePlaybackTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard await deps.playback.pausePlayback() else {
            return toolError("Playback is unavailable.")
        }
        return await actionTool(op: "pause_result", payload: [:])
            ?? toolError("pause_playback result shaping is unavailable")
    }

    static func setPlaybackRateTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let plan = await actionPlan(PlaybackRatePlan.self, op: "rate_plan", args: args) else {
            return toolError("set_playback_rate planning is unavailable")
        }
        if let error = plan.error { return toolError(error) }
        guard let requested = plan.rate else { return toolError("set_playback_rate plan was incomplete") }
        guard let applied = await deps.playback.setPlaybackRate(requested) else {
            return toolError("Playback is unavailable.")
        }
        return await actionTool(op: "rate_result", payload: [
            "requested_rate": requested,
            "rate": applied,
        ]) ?? toolError("set_playback_rate result shaping is unavailable")
    }

    static func setSleepTimerTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let plan = await actionPlan(SleepTimerPlan.self, op: "sleep_plan", args: args) else {
            return toolError("set_sleep_timer planning is unavailable")
        }
        if let error = plan.error { return toolError(error) }
        guard let mode = plan.mode else { return toolError("set_sleep_timer plan was incomplete") }
        guard let label = await deps.playback.setSleepTimer(mode: mode, minutes: plan.minutes) else {
            return toolError("Playback is unavailable.")
        }
        var payload: [String: Any] = [
            "mode": mode,
            "label": label,
        ]
        if let minutes = plan.minutes { payload["minutes"] = minutes }
        return await actionTool(op: "sleep_result", payload: payload)
            ?? toolError("set_sleep_timer result shaping is unavailable")
    }

    // MARK: - Playback navigation

    static func getNowPlayingTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        let state = await deps.playback.getNowPlaying()
        var payload: [String: Any] = [
            "is_playing": state.isPlaying,
            "position_seconds": state.positionSeconds,
            "rate": state.rate,
        ]
        if let id = state.episodeID { payload["episode_id"] = id }
        if let title = state.episodeTitle { payload["episode_title"] = title }
        if let pid = state.podcastID { payload["podcast_id"] = pid }
        if let ptitle = state.podcastTitle { payload["podcast_title"] = ptitle }
        if let dur = state.durationSeconds { payload["duration_seconds"] = dur }
        return await actionTool(op: "now_playing_result", payload: payload)
            ?? toolError("get_now_playing result shaping is unavailable")
    }

    static func seekToTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let plan = await actionPlan(SeekPlan.self, op: "seek_plan", args: args) else {
            return toolError("seek_to planning is unavailable")
        }
        if let error = plan.error { return toolError(error) }
        guard let position = plan.positionSeconds else { return toolError("seek_to plan was incomplete") }
        guard let applied = await deps.playback.seekTo(positionSeconds: position) else {
            return toolError("seek_to failed: nothing is currently loaded")
        }
        return await actionTool(op: "seek_result", payload: ["position_seconds": applied])
            ?? toolError("seek_to result shaping is unavailable")
    }

    static func skipForwardTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        let seconds = podcastActionNumericArg(args["seconds"])
        guard let newPosition = await deps.playback.skipForward(seconds: seconds) else {
            return toolError("skip_forward failed: nothing is currently loaded")
        }
        var payload: [String: Any] = ["new_position_seconds": newPosition]
        if let s = seconds { payload["skipped_seconds"] = s }
        return await actionTool(op: "skip_result", payload: payload)
            ?? toolError("skip_forward result shaping is unavailable")
    }

    static func skipBackwardTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        let seconds = podcastActionNumericArg(args["seconds"])
        guard let newPosition = await deps.playback.skipBackward(seconds: seconds) else {
            return toolError("skip_backward failed: nothing is currently loaded")
        }
        var payload: [String: Any] = ["new_position_seconds": newPosition]
        if let s = seconds { payload["skipped_seconds"] = s }
        return await actionTool(op: "skip_result", payload: payload)
            ?? toolError("skip_backward result shaping is unavailable")
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
        guard let plan = await actionPlan(ActionIDPlan.self, op: "episode_id_plan", args: args) else {
            return toolError("\(action) planning is unavailable")
        }
        if let error = plan.error { return toolError(error) }
        guard let episodeID = plan.episodeID else { return toolError("\(action) plan was incomplete") }
        do {
            return await actionTool(
                op: "episode_mutation_result",
                payload: rawEpisodeMutation(try await mutate(episodeID))
            ) ?? toolError("\(action) result shaping is unavailable")
        } catch {
            return toolError("\(action) failed: \(error.localizedDescription)")
        }
    }

    // MARK: - Transcript + feed

    static func requestTranscriptionTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let plan = await actionPlan(ActionIDPlan.self, op: "episode_id_plan", args: args) else {
            return toolError("request_transcription planning is unavailable")
        }
        if let error = plan.error { return toolError(error) }
        guard let episodeID = plan.episodeID else { return toolError("request_transcription plan was incomplete") }
        do {
            let result = try await deps.library.requestTranscription(episodeID: episodeID)
            return await actionTool(op: "transcript_result", payload: rawTranscriptResult(result))
                ?? toolError("request_transcription result shaping is unavailable")
        } catch {
            return toolError("request_transcription failed: \(error.localizedDescription)")
        }
    }

    static func downloadAndTranscribeTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let plan = await actionPlan(DownloadTranscribePlan.self, op: "download_transcribe_plan", args: args) else {
            return toolError("download_and_transcribe planning is unavailable")
        }
        if let error = plan.error { return toolError(error) }
        if plan.source == "external", let audioURL = plan.audioURL, let feedURL = plan.feedURL {
            return await downloadAndTranscribeExternalTool(
                feedURLString: feedURL,
                audioURLString: audioURL,
                deps: deps
            )
        }
        guard plan.source == "library", let episodeID = plan.episodeID else {
            return toolError("download_and_transcribe plan was incomplete")
        }
        guard await deps.fetcher.episodeMetadata(episodeID: episodeID) != nil else {
            return toolError("download_and_transcribe: episode '\(episodeID)' not found in library")
        }
        do {
            let result = try await deps.library.downloadAndTranscribe(episodeID: episodeID)
            return await actionTool(op: "transcript_result", payload: rawTranscriptResult(result))
                ?? toolError("download_and_transcribe result shaping is unavailable")
        } catch {
            return toolError("download_and_transcribe failed: \(error.localizedDescription)")
        }
    }

    static func refreshFeedTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let plan = await actionPlan(ActionIDPlan.self, op: "podcast_id_plan", args: args) else {
            return toolError("refresh_feed planning is unavailable")
        }
        if let error = plan.error { return toolError(error) }
        guard let podcastID = plan.podcastID else { return toolError("refresh_feed plan was incomplete") }
        do {
            let result = try await deps.library.refreshFeed(podcastID: podcastID)
            return await actionTool(op: "refresh_result", payload: rawRefreshResult(result))
                ?? toolError("refresh_feed result shaping is unavailable")
        } catch {
            return toolError("refresh_feed failed: \(error.localizedDescription)")
        }
    }

    // MARK: - Clipping

    static func createClipTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let plan = await actionPlan(ClipActionPlan.self, op: "clip_plan", args: args) else {
            return toolError("create_clip planning is unavailable")
        }
        if let error = plan.error { return toolError(error) }
        guard let episodeID = plan.episodeID,
              let startSeconds = plan.startSeconds,
              let endSeconds = plan.endSeconds
        else { return toolError("create_clip plan was incomplete") }
        guard await deps.fetcher.episodeMetadata(episodeID: episodeID) != nil else {
            return toolError("create_clip: episode '\(episodeID)' not found in library")
        }
        guard startSeconds < endSeconds else {
            return toolError("create_clip: start_seconds (\(startSeconds)) must be less than end_seconds (\(endSeconds))")
        }
        do {
            let result = try await deps.library.createClip(
                episodeID: episodeID,
                startSeconds: startSeconds,
                endSeconds: endSeconds,
                caption: plan.caption,
                transcriptText: plan.transcriptText
            )
            return await actionTool(op: "clip_result", payload: rawClipResult(result))
                ?? toolError("create_clip result shaping is unavailable")
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
        guard let plan = await actionPlan(PlayEpisodePlan.self, op: "play_plan", args: args) else {
            return toolError("play_episode planning is unavailable")
        }
        if let error = plan.error { return toolError(error) }
        guard let position = QueuePosition(rawValue: plan.queuePosition ?? QueuePosition.now.rawValue) else {
            return toolError("play_episode plan returned an invalid queue position")
        }
        if plan.source == "library", let episodeID = plan.episodeID {
            return await playLibraryEpisode(
                episodeID: episodeID,
                startSeconds: plan.startSeconds,
                endSeconds: plan.endSeconds,
                position: position,
                deps: deps
            )
        }
        guard plan.source == "external", let audioURLString = plan.audioURL else {
            return toolError("play_episode plan was incomplete")
        }
        return await playExternalAudioURL(
            audioURLString: audioURLString,
            title: plan.title,
            feedURLString: plan.feedURL,
            durationSeconds: plan.durationSeconds,
            startSeconds: plan.startSeconds,
            endSeconds: plan.endSeconds,
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
        guard await deps.fetcher.episodeMetadata(episodeID: episodeID) != nil else {
            return toolError("play_episode: episode '\(episodeID)' not found in library")
        }
        switch await deps.playback.playEpisode(
            episodeID: episodeID,
            startSeconds: startSeconds,
            endSeconds: endSeconds,
            queuePosition: position
        ) {
        case let .played(result):
            return await playResultEnvelope(result, startSeconds: startSeconds, endSeconds: endSeconds)
        case let .rejected(message):
            return toolError("play_episode rejected: \(message)")
        case .unavailable:
            return toolError("play_episode failed: playback host unavailable")
        }
    }

    private static func playExternalAudioURL(
        audioURLString: String,
        title: String?,
        feedURLString: String?,
        durationSeconds: Double?,
        startSeconds: Double?,
        endSeconds: Double?,
        position: QueuePosition,
        deps: PodcastAgentToolDeps
    ) async -> String {
        guard let audioURL = URL(string: audioURLString) else {
            return toolError("Invalid 'audio_url': \(audioURLString)")
        }
        guard let title else { return toolError("play_episode plan was incomplete") }
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
        var payload = rawPlayEpisodeResult(result, startSeconds: startSeconds, endSeconds: endSeconds)
        payload["audio_url"] = audioURLString
        payload["title"] = title
        if let feedURLString { payload["feed_url"] = feedURLString }
        return await actionTool(op: "play_result", payload: payload)
            ?? toolError("play_episode result shaping is unavailable")
    }

    // MARK: - Private result helpers (callers all in this file)

    private static func playResultEnvelope(
        _ result: PlayEpisodeResult,
        startSeconds: Double?,
        endSeconds: Double?
    ) async -> String {
        await actionTool(
            op: "play_result",
            payload: rawPlayEpisodeResult(result, startSeconds: startSeconds, endSeconds: endSeconds)
        ) ?? toolError("play_episode result shaping is unavailable")
    }

    // MARK: - Private planning + arg helpers (file-scoped, all callers in this file)

    private static func actionPlan<T: Decodable>(
        _ type: T.Type,
        op: String,
        args: [String: Any]
    ) async -> T? {
        // Serialize synchronously so the non-Sendable `args` dict is not sent
        // across the `await` into `actionToolJSON`.
        var request = args
        request["op"] = op
        guard let data = try? JSONSerialization.data(withJSONObject: request),
              let json = String(data: data, encoding: .utf8)
        else { return nil }
        guard let envelope = await actionToolJSON(json),
              let data = envelope.data(using: .utf8)
        else { return nil }
        return try? JSONDecoder().decode(T.self, from: data)
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
