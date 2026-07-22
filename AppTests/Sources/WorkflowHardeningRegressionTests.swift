import Foundation
import XCTest
@testable import Podcastr

final class WorkflowRepairRegressionTests: XCTestCase {
    func testSwiftCoordinatorCannotCreateMigratedWorkflowWork() {
        XCTAssertFalse(JobStore.supportedKindSQL.contains("transcriptIngest"))
        XCTAssertFalse(JobStore.supportedKindSQL.contains("transcriptIndex"))
        XCTAssertFalse(JobStore.supportedKindSQL.contains("scheduledAgentRun"))
    }
}
