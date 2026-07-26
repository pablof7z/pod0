import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class CoreAgentStreamingStateTests: XCTestCase {
    func testBurstUpdatesCoalesceToLatestBoundedValue() {
        let state = CoreAgentStreamingState()
        let turnID = AgentTurnId(high: 1, low: 2)
        let fenceID = AgentExecutionFenceId(high: 3, low: 4)

        state.begin(turnID: turnID, fenceID: fenceID, maximumBytes: 16)
        state.update(turnID: turnID, fenceID: fenceID, content: "one")
        state.update(turnID: turnID, fenceID: fenceID, content: "latest")

        XCTAssertEqual(state.content, "")
        XCTAssertTrue(state.isActive)

        state.finish(turnID: turnID, fenceID: fenceID)

        XCTAssertEqual(state.content, "latest")
        XCTAssertFalse(state.isActive)
    }

    func testStaleFenceAndOversizedUpdatesCannotReplaceActivePresentation() {
        let state = CoreAgentStreamingState()
        let turnID = AgentTurnId(high: 1, low: 2)
        let fenceID = AgentExecutionFenceId(high: 3, low: 4)
        let staleFenceID = AgentExecutionFenceId(high: 9, low: 9)

        state.begin(turnID: turnID, fenceID: fenceID, maximumBytes: 8)
        state.update(turnID: turnID, fenceID: fenceID, content: "valid")
        state.update(turnID: turnID, fenceID: staleFenceID, content: "stale")
        state.update(turnID: turnID, fenceID: fenceID, content: "too-large")
        state.finish(turnID: turnID, fenceID: staleFenceID)

        XCTAssertEqual(state.content, "")
        XCTAssertTrue(state.isActive)

        state.finish(turnID: turnID, fenceID: fenceID)

        XCTAssertEqual(state.content, "valid")
        XCTAssertFalse(state.isActive)
    }

    func testNewFenceCancelsPendingPresentationFromPriorTurn() {
        let state = CoreAgentStreamingState()
        let firstTurn = AgentTurnId(high: 1, low: 2)
        let firstFence = AgentExecutionFenceId(high: 3, low: 4)
        let secondTurn = AgentTurnId(high: 5, low: 6)
        let secondFence = AgentExecutionFenceId(high: 7, low: 8)

        state.begin(turnID: firstTurn, fenceID: firstFence, maximumBytes: 16)
        state.update(turnID: firstTurn, fenceID: firstFence, content: "obsolete")
        state.begin(turnID: secondTurn, fenceID: secondFence, maximumBytes: 16)
        state.finish(turnID: firstTurn, fenceID: firstFence)

        XCTAssertEqual(state.turnID, secondTurn)
        XCTAssertEqual(state.content, "")
        XCTAssertTrue(state.isActive)

        state.update(turnID: secondTurn, fenceID: secondFence, content: "current")
        state.finish(turnID: secondTurn, fenceID: secondFence)

        XCTAssertEqual(state.content, "current")
        XCTAssertFalse(state.isActive)
    }
}
