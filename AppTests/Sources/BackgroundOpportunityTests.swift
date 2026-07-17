import XCTest
@testable import Podcastr

@MainActor
final class BackgroundOpportunityTests: XCTestCase {
    func testSuccessResubmitsAndCompletesExactlyOnce() async {
        var resubmissions = 0
        var completions: [Bool] = []
        let opportunity = BackgroundOpportunity(
            resubmit: { resubmissions += 1 },
            complete: { completions.append($0) },
            cancel: {}
        )
        opportunity.start { true }
        try? await Task.sleep(for: .milliseconds(20))
        opportunity.expire()

        XCTAssertEqual(resubmissions, 1)
        XCTAssertEqual(completions, [true])
    }

    func testExpirationCancelsAndCompletesFailureExactlyOnce() async {
        var resubmissions = 0
        var completions: [Bool] = []
        var cancellations = 0
        let opportunity = BackgroundOpportunity(
            resubmit: { resubmissions += 1 },
            complete: { completions.append($0) },
            cancel: { cancellations += 1 }
        )
        opportunity.start {
            try? await Task.sleep(for: .seconds(30))
            return true
        }
        opportunity.expire()
        try? await Task.sleep(for: .milliseconds(20))
        opportunity.expire()

        XCTAssertEqual(resubmissions, 1)
        XCTAssertEqual(cancellations, 1)
        XCTAssertEqual(completions, [false])
    }
}
