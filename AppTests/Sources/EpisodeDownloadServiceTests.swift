import XCTest
@testable import Podcastr

@MainActor
final class EpisodeDownloadServiceTests: XCTestCase {

    func testBackgroundURLSessionCompletionHandlerRunsWhenSessionFinishesEvents() {
        let service = EpisodeDownloadService.shared
        var didCallCompletion = false

        service.handleEventsForBackgroundURLSession(
            identifier: EpisodeDownloadService.backgroundSessionIdentifier
        ) {
            didCallCompletion = true
        }
        service.handleBackgroundEventsFinished(for: service.session)

        XCTAssertTrue(didCallCompletion)
    }

    func testUnknownBackgroundURLSessionIdentifierCompletesImmediately() {
        let service = EpisodeDownloadService.shared
        var didCallCompletion = false

        service.handleEventsForBackgroundURLSession(identifier: "other.session") {
            didCallCompletion = true
        }

        XCTAssertTrue(didCallCompletion)
    }
}
