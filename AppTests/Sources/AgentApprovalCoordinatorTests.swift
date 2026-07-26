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

        let decision = await task.value
        XCTAssertEqual(decision, .approve)
        XCTAssertNil(coordinator.current)
    }

    func testExplicitDenialRemainsDistinctFromDismissal() async throws {
        let coordinator = AgentApprovalCoordinator()
        let task = Task { @MainActor in await coordinator.requestApproval(approvalRequest()) }
        await Task.yield()
        let pending = try XCTUnwrap(coordinator.current)

        coordinator.deny(pending.id)

        let decision = await task.value
        XCTAssertEqual(decision, .deny)
        XCTAssertNil(coordinator.current)
    }

    func testCancellationDismissesAndReleasesPresentation() async throws {
        let coordinator = AgentApprovalCoordinator()
        let task = Task { @MainActor in await coordinator.requestApproval(approvalRequest()) }
        await Task.yield()
        XCTAssertNotNil(coordinator.current)

        task.cancel()
        let decision = await task.value

        XCTAssertEqual(decision, .dismiss)
        XCTAssertNil(coordinator.current)
    }

    func testOnlyTheActivePresentationContextClaimsAnApproval() async throws {
        let coordinator = AgentApprovalCoordinator()
        let task = Task { @MainActor in await coordinator.requestApproval(approvalRequest()) }
        await Task.yield()

        let hiddenPresenter = AgentApprovalPresenter(
            coordinator: coordinator,
            isEnabled: false
        )
        let visiblePresenter = AgentApprovalPresenter(
            coordinator: coordinator,
            isEnabled: true
        )

        XCTAssertNil(hiddenPresenter.pendingForPresentation)
        let pending = try XCTUnwrap(visiblePresenter.pendingForPresentation)
        coordinator.dismiss(pending.id)
        let decision = await task.value
        XCTAssertEqual(decision, .dismiss)
    }
}
