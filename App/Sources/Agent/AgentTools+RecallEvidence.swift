import Foundation
import Pod0Core

extension AgentTools {
    static func serializeRecallEvidence(_ item: RecallEvidenceProjection) -> [String: Any] {
        var provenance: [String: Any] = [
            "source": item.provenance.source.stableName,
            "source_payload_digest": item.provenance.sourcePayloadDigest.stableString,
        ]
        if let provider = item.provenance.provider { provenance["provider"] = provider }
        var score: [String: Any] = [
            "vector_rrf_units": NSNumber(value: item.score.vectorRrfUnits),
            "lexical_rrf_units": NSNumber(value: item.score.lexicalRrfUnits),
            "total_rrf_units": NSNumber(value: item.score.totalRrfUnits),
            "base_rank": item.score.baseRank,
        ]
        if let rerankRank = item.score.rerankRank { score["rerank_rank"] = rerankRank }
        var row: [String: Any] = [
            "episode_id": item.episodeId.uuid?.uuidString ?? "",
            "podcast_id": item.podcastId.uuid?.uuidString ?? "",
            "generation_id": item.generationId.stableString,
            "transcript_version_id": item.transcriptVersionId.stableString,
            "transcript_content_digest": item.transcriptContentDigest.stableString,
            "span_id": item.spanId.stableString,
            "first_segment_id": item.firstSegmentId.stableString,
            "last_segment_id": item.lastSegmentId.stableString,
            "start_segment_ordinal": item.startSegmentOrdinal,
            "end_segment_ordinal_exclusive": item.endSegmentOrdinalExclusive,
            "start_milliseconds": NSNumber(value: item.startMilliseconds),
            "end_milliseconds": NSNumber(value: item.endMilliseconds),
            "start_seconds": Double(item.startMilliseconds) / 1_000,
            "end_seconds": Double(item.endMilliseconds) / 1_000,
            "text": item.excerpt,
            "provenance": provenance,
            "score": score,
        ]
        if let speaker = item.speakerId { row["speaker_id"] = speaker.stableString }
        return row
    }
}
