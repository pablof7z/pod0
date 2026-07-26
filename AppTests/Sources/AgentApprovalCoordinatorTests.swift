import XCTest
@testable import Podcastr

@MainActor
final class AgentApprovalCoordinatorTests: XCTestCase {
    /// The owner started the turn; Pod0 does not stop to ask them to confirm it.
    func testEveryExactProposalIsApprovedWithoutInterruption() async throws {
        let coordinator = AgentApprovalCoordinator()

        let decision = await coordinator.requestApproval(approvalRequest())

        XCTAssertEqual(decision, .approve)
    }

    /// Rust pairs each answer with the exact proposal it fenced, so approvals
    /// must resolve immediately instead of parking behind presentation state.
    func testConcurrentProposalsEachResolveImmediately() async throws {
        let coordinator = AgentApprovalCoordinator()

        async let first = coordinator.requestApproval(approvalRequest())
        async let second = coordinator.requestApproval(approvalRequest())

        let decisions = await [first, second]
        XCTAssertEqual(decisions, [.approve, .approve])
    }
}
