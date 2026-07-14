import XCTest
@testable import Podcastr

/// Locks the wall-clock representation rendered in the agent Run Logs
/// list and detail surfaces. Past 60s the formatter switches from
/// "X.Ys" to "Mm Ss" so a 2-minute run reads as "2m 0s" instead of
/// the ambiguous "120.0s" the surface used to show.
final class AgentRunFormatTests: XCTestCase {

    func testSubSecondShowsMs() {
        XCTAssertEqual(AgentRunFormat.duration(0), "0ms")
        XCTAssertEqual(AgentRunFormat.duration(1), "1ms")
        XCTAssertEqual(AgentRunFormat.duration(999), "999ms")
    }

    func testSecondsBelowOneMinuteUseDecimal() {
        XCTAssertEqual(AgentRunFormat.duration(1_000), "1.0s")
        XCTAssertEqual(AgentRunFormat.duration(1_500), "1.5s")
        XCTAssertEqual(AgentRunFormat.duration(59_900), "59.9s")
    }

    func testExactlyOneMinuteSwitchesToMinuteRepresentation() {
        // 60s is the boundary — under previous behaviour this was
        // "60.0s"; the new formatter favours minute granularity once
        // the run hits the minute mark.
        XCTAssertEqual(AgentRunFormat.duration(60_000), "1m 0s")
    }

    func testMinutesShowMinutesAndSeconds() {
        XCTAssertEqual(AgentRunFormat.duration(75_000), "1m 15s")
        XCTAssertEqual(AgentRunFormat.duration(120_000), "2m 0s")
        XCTAssertEqual(AgentRunFormat.duration(599_000), "9m 59s")
    }

    func testHoursShowHoursAndMinutes() {
        // 1h0m boundary — drops the seconds segment because at this
        // scale they're noise next to wall-clock minutes.
        XCTAssertEqual(AgentRunFormat.duration(3_600_000), "1h 0m")
        XCTAssertEqual(AgentRunFormat.duration(3_660_000), "1h 1m")
        XCTAssertEqual(AgentRunFormat.duration(7_200_000), "2h 0m")
    }
}
