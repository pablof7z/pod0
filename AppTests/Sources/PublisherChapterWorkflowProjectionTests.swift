import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class PublisherChapterWorkflowProjectionTests: XCTestCase {
    func testRequestedRustWorkflowRendersAsCancellableNativeStatus() {
        let projection = WorkflowJobProjection(
            publisherChapterWorkflow: workflow(stage: .requested, canCancel: true)
        )

        XCTAssertEqual(projection.kind, .publisherChapters)
        XCTAssertEqual(projection.state, .running)
        XCTAssertEqual(projection.authority, .sharedRustPublisherChapters)
        XCTAssertEqual(projection.coreWorkflowRevision, 7)
        XCTAssertEqual(projection.allowedActions, [.cancel])
        XCTAssertEqual(WorkflowPresentationCopy.title(for: projection), "Fetching chapters")
    }

    func testFailedRustWorkflowRendersItsTypedRetryAction() {
        let projection = WorkflowJobProjection(publisherChapterWorkflow: workflow(
            stage: .failed,
            failure: PublisherChapterWorkflowFailure(
                code: .offline,
                safeDetail: "Network unavailable",
                retryable: false
            ),
            canRetry: true
        ))

        XCTAssertEqual(projection.state, .failedPermanent)
        XCTAssertEqual(projection.lastErrorClass, .offline)
        XCTAssertEqual(projection.lastErrorMessage, "Network unavailable")
        XCTAssertEqual(projection.allowedActions, [.retry])
    }

    private func workflow(
        stage: PublisherChapterWorkflowStage,
        failure: PublisherChapterWorkflowFailure? = nil,
        canRetry: Bool = false,
        canCancel: Bool = false
    ) -> PublisherChapterWorkflowProjection {
        PublisherChapterWorkflowProjection(
            episodeId: EpisodeId(high: 1, low: 2),
            sourceVersion: "source-v1",
            stage: stage,
            workflowRevision: StateRevision(value: 7),
            attempt: 2,
            maxAttempts: 5,
            requestId: HostRequestId(high: 3, low: 4),
            cancellationId: CancellationId(high: 5, low: 6),
            notBefore: UnixTimestampMilliseconds(value: 2_000),
            selectedArtifactId: nil,
            failure: failure,
            createdAt: UnixTimestampMilliseconds(value: 1_000),
            updatedAt: UnixTimestampMilliseconds(value: 2_000),
            canRetry: canRetry,
            canCancel: canCancel
        )
    }
}
