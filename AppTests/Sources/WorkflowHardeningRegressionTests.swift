import Foundation
import XCTest
@testable import Podcastr

final class WorkflowRepairRegressionTests: XCTestCase {
    func testSwiftPlannerCannotCreateTranscriptWork() {
        let jobs = DesiredStatePlanner().plan(.init(
            settings: Settings(),
            scheduledTasks: [],
            now: Date()
        ))

        XCTAssertTrue(jobs.isEmpty)
        XCTAssertFalse(JobStore.supportedKindSQL.contains("transcriptIngest"))
        XCTAssertFalse(JobStore.supportedKindSQL.contains("transcriptIndex"))
    }
}
