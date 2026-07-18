import Foundation
import os.log

enum RecallQualityLogger {
    private static let logger = Logger.app("RecallQuality")

    static func outcome(
        status: RecallAnswer.Status,
        evidenceCount: Int,
        duration: Duration
    ) {
        logger.info(
            "recall_outcome status=\(status.rawValue, privacy: .public) evidence_count=\(evidenceCount, privacy: .public) latency=\(latencyBucket(duration), privacy: .public)"
        )
    }

    static func citationTapped() {
        logger.info("recall_citation_tapped")
    }

    private static func latencyBucket(_ duration: Duration) -> String {
        let milliseconds = duration.components.seconds * 1_000
            + duration.components.attoseconds / 1_000_000_000_000_000
        return switch milliseconds {
        case ..<250: "under_250ms"
        case ..<750: "250_749ms"
        case ..<2_000: "750_1999ms"
        default: "2000ms_plus"
        }
    }
}
