import XCTest
@testable import Podcastr

final class TranscriptCredentialWorkflowSignalTests: XCTestCase {
    func testCredentialSignalPublishesDependencyChangeNotification() async {
        let expectation = expectation(
            forNotification: TranscriptCredentialWorkflowSignal.notification,
            object: nil
        )

        TranscriptCredentialWorkflowSignal.send()

        await fulfillment(of: [expectation], timeout: 1)
    }
}
