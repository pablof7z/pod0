import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class ModelChapterWorkflowProjectionTests: XCTestCase {
    func testProviderAcceptedWorkflowRendersAsRustOwnedModelWork() {
        let projection = WorkflowJobProjection(modelChapterWorkflow: workflow(
            stage: .providerAccepted,
            actions: ModelChapterWorkflowAllowedActions(canRetry: false, canCancel: true)
        ))

        XCTAssertEqual(projection.kind, .chapterArtifacts)
        XCTAssertEqual(projection.state, .running)
        XCTAssertEqual(projection.resourceClass, .utilityLLM)
        XCTAssertEqual(projection.authority, .sharedRustModelChapters)
        XCTAssertEqual(projection.coreWorkflowRevision, 7)
        XCTAssertEqual(projection.externalProvider, "openrouter")
        XCTAssertEqual(projection.externalOperationState, "providerAccepted")
        XCTAssertEqual(projection.allowedActions, [.cancel])
    }

    func testAmbiguousWorkflowNeverOffersUnsafeImplicitRetry() {
        let projection = WorkflowJobProjection(modelChapterWorkflow: workflow(
            stage: .ambiguous,
            failure: ModelChapterWorkflowFailure(
                code: .ambiguousSubmission,
                safeDetail: "Provider submission outcome is unknown",
                retry: .explicitOnly,
                mayHaveSubmitted: true
            ),
            actions: ModelChapterWorkflowAllowedActions(canRetry: false, canCancel: true)
        ))

        XCTAssertEqual(projection.state, .blocked)
        XCTAssertEqual(projection.lastErrorClass, .unsafeToRetry)
        XCTAssertEqual(projection.allowedActions, [.cancel])
    }

    func testSucceededWorkflowProjectsSelectedArtifact() {
        let artifactID = ChapterArtifactId(high: 11, low: 12)
        let projection = WorkflowJobProjection(modelChapterWorkflow: workflow(
            stage: .succeeded,
            selectedArtifactID: artifactID
        ))

        XCTAssertEqual(projection.state, .succeeded)
        XCTAssertEqual(projection.outputVersion, artifactID.stableString)
        XCTAssertTrue(projection.allowedActions.isEmpty)
    }

    private func workflow(
        stage: ModelChapterWorkflowStage,
        selectedArtifactID: ChapterArtifactId? = nil,
        failure: ModelChapterWorkflowFailure? = nil,
        actions: ModelChapterWorkflowAllowedActions = .init(canRetry: false, canCancel: false)
    ) -> ModelChapterWorkflowProjection {
        ModelChapterWorkflowProjection(
            episodeId: EpisodeId(high: 1, low: 2),
            configuredModel: "openrouter:provider/model",
            mode: .generate,
            sourceVersion: "source-v1",
            stage: stage,
            workflowRevision: StateRevision(value: 7),
            generation: 3,
            attempt: 2,
            maxAttempts: 8,
            requestId: HostRequestId(high: 3, low: 4),
            cancellationId: CancellationId(high: 5, low: 6),
            notBefore: UnixTimestampMilliseconds(value: 2_000),
            selectedArtifactId: selectedArtifactID,
            failure: failure,
            replanPending: false,
            mayHaveSubmitted: failure?.mayHaveSubmitted ?? false,
            createdAt: UnixTimestampMilliseconds(value: 1_000),
            updatedAt: UnixTimestampMilliseconds(value: 2_000),
            allowedActions: actions
        )
    }
}
