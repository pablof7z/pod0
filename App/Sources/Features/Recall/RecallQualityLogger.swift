import Foundation
import os.log

enum RecallQualityLogger {
    private static let logger = Logger.app("RecallQuality")

    static func citationTapped() {
        logger.info("recall_citation_tapped")
    }

}
