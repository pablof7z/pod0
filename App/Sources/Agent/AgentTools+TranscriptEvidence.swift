import Foundation

extension AgentTools {
    static func serializeTranscriptHit(_ hit: TranscriptHit) -> [String: Any] {
        var row: [String: Any] = [
            "episode_id": hit.episodeID,
            "start_seconds": hit.startSeconds,
            "end_seconds": hit.endSeconds,
            "text": hit.text,
        ]
        if let chunkID = hit.chunkID { row["chunk_id"] = chunkID }
        if let podcastID = hit.podcastID { row["podcast_id"] = podcastID }
        if let version = hit.artifactVersion { row["artifact_version"] = version }
        if let provenance = hit.provenance { row["provenance"] = provenance }
        if let speaker = hit.speaker { row["speaker"] = speaker }
        if let score = hit.score { row["score"] = score }
        return row
    }
}
