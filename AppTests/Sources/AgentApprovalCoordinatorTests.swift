import XCTest
@testable import Podcastr

@MainActor
final class AgentApprovalCoordinatorTests: XCTestCase {
    func testApprovalResolvesExactQueuedRequest() async throws {
        let coordinator = AgentApprovalCoordinator()
        let request = approvalRequest()
        let task = Task { @MainActor in await coordinator.requestApproval(request) }
        await Task.yield()
        let pending = try XCTUnwrap(coordinator.current)
        XCTAssertEqual(pending.request, request)

        coordinator.approve(pending.id)

        let approved = await task.value
        XCTAssertTrue(approved)
        XCTAssertNil(coordinator.current)
    }

    func testCancellationDeniesAndReleasesPresentation() async throws {
        let coordinator = AgentApprovalCoordinator()
        let task = Task { @MainActor in await coordinator.requestApproval(approvalRequest()) }
        await Task.yield()
        XCTAssertNotNil(coordinator.current)

        task.cancel()
        let approved = await task.value

        XCTAssertFalse(approved)
        XCTAssertNil(coordinator.current)
    }
}
