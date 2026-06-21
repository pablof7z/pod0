import Foundation

// MARK: - Podcast action raw-result helpers

extension AgentTools {
    /// Shared payload shape for both `play_episode` branches (library and
    /// external) so the LLM sees a consistent success envelope.
    static func rawPlayEpisodeResult(
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
        return payload
    }

    static func rawTranscriptResult(_ result: TranscriptRequestResult) -> [String: Any] {
        var payload: [String: Any] = [
            "episode_id": result.episodeID,
            "status": result.status,
        ]
        if let source = result.source { payload["source"] = source }
        if let message = result.message { payload["message"] = message }
        return payload
    }

    static func rawEpisodeMutation(_ result: EpisodeMutationResult) -> [String: Any] {
        var payload: [String: Any] = [
            "episode_id": result.episodeID,
            "episode_title": result.episodeTitle,
            "state": result.state,
        ]
        if let podcastID = result.podcastID { payload["podcast_id"] = podcastID }
        if let podcastTitle = result.podcastTitle { payload["podcast_title"] = podcastTitle }
        return payload
    }

    static func rawRefreshResult(_ result: FeedRefreshResult) -> [String: Any] {
        var payload: [String: Any] = [
            "podcast_id": result.podcastID,
            "title": result.title,
            "episode_count": result.episodeCount,
            "new_episode_count": result.newEpisodeCount,
        ]
        if let refreshedAt = result.refreshedAt {
            payload["refreshed_at"] = Int(refreshedAt.timeIntervalSince1970)
        }
        return payload
    }

    static func rawClipResult(_ result: ClipResult) -> [String: Any] {
        var payload: [String: Any] = [
            "clip_id": result.clipID,
            "episode_id": result.episodeID,
            "episode_title": result.episodeTitle,
            "start_seconds": result.startSeconds,
            "end_seconds": result.endSeconds,
        ]
        if !result.transcriptText.isEmpty { payload["transcript_text"] = result.transcriptText }
        if let caption = result.caption { payload["caption"] = caption }
        if let podcastID = result.podcastID { payload["podcast_id"] = podcastID }
        return payload
    }
}
