import Foundation

// MARK: - YouTube tool handlers

extension AgentTools {

    static func searchYouTubeTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let query = (args["query"] as? String)?.trimmingCharacters(in: .whitespacesAndNewlines),
              !query.isEmpty else {
            return toolError("Missing or empty 'query'")
        }
        let limit = {
            if let i = args["limit"] as? Int { return max(1, min(20, i)) }
            if let d = args["limit"] as? Double { return max(1, min(20, Int(d))) }
            return 5
        }()

        do {
            let results = try await deps.youtubeIngestion.searchVideos(query: query, limit: limit)
            let rows = results.map { r -> [String: Any] in
                var row: [String: Any] = ["url": r.url, "title": r.title, "author": r.author]
                if let d = r.durationSeconds { row["duration_seconds"] = d }
                return row
            }
            return toolSuccess(["query": query, "total_found": rows.count, "results": rows])
        } catch {
            return toolError("search_youtube failed: \(error.localizedDescription)")
        }
    }

    static func ingestYouTubeVideoTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let youtubeURL = (args["url"] as? String)?.trimmingCharacters(in: .whitespacesAndNewlines),
              !youtubeURL.isEmpty else {
            return toolError("Missing or empty 'url'")
        }
        let customTitle = (args["title"] as? String)?.trimmingCharacters(in: .whitespacesAndNewlines).nilIfEmpty
        let transcribe = (args["transcribe"] as? Bool) ?? true

        do {
            let result = try await deps.youtubeIngestion.ingestVideo(
                youtubeURL: youtubeURL,
                customTitle: customTitle,
                transcribe: transcribe
            )
            var payload: [String: Any] = [
                "episode_id": result.episodeID,
                "title": result.title,
                "author": result.author,
            ]
            if let dur = result.durationSeconds { payload["duration_seconds"] = dur }
            if let ts = result.transcriptStatus { payload["transcript_status"] = ts }
            payload["message"] = transcribe
                ? "YouTube video ingested and transcription queued."
                : "YouTube video ingested."
            return toolSuccess(payload)
        } catch {
            return toolError("ingest_youtube_video failed: \(error.localizedDescription)")
        }
    }
}
