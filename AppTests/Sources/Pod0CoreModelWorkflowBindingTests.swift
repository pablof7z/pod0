import Pod0Core
import XCTest

final class Pod0CoreModelWorkflowBindingTests: XCTestCase {
    func testGeneratedModelWorkflowBoundaryIsTypedEndToEnd() {
        let episodeID = EpisodeId(high: 1, low: 2)
        let fence = ChapterModelSubmissionFenceId(high: 3, low: 4)
        let requestID = HostRequestId(high: 5, low: 6)
        let cancellationID = CancellationId(high: 7, low: 8)
        let execution = ChapterModelExecutionRequest(
            provider: "openrouter",
            model: "model-a",
            systemPrompt: "Return JSON.",
            userPrompt: "Transcript evidence",
            responseFormat: .jsonObject,
            maximumCompletionBytes: 65_536
        )
        let execute = HostRequest.executeChapterModel(
            episodeId: episodeID,
            generation: 2,
            submissionFenceId: fence,
            execution: execution
        )
        let recover = HostRequest.recoverChapterModelOperation(
            episodeId: episodeID,
            generation: 2,
            submissionFenceId: fence,
            provider: "openrouter",
            model: "model-a",
            providerOperationId: "operation-1",
            providerStatus: "running",
            maximumCompletionBytes: 65_536
        )
        let wakeReason = CoreWakeReason.modelChapterRetry(
            episodeId: episodeID,
            generation: 2,
            submissionFenceId: fence
        )
        let wake = HostRequest.scheduleCoreWake(
            wakeAt: UnixTimestampMilliseconds(value: 1_800_000_030_000),
            reason: wakeReason
        )

        XCTAssertEqual(execute, .executeChapterModel(
            episodeId: episodeID,
            generation: 2,
            submissionFenceId: fence,
            execution: execution
        ))
        XCTAssertNotEqual(execute, recover)
        XCTAssertEqual(wake, .scheduleCoreWake(
            wakeAt: UnixTimestampMilliseconds(value: 1_800_000_030_000),
            reason: wakeReason
        ))

        let provider = HostObservation.chapterModelProviderAccepted(
            episodeId: episodeID,
            generation: 2,
            submissionFenceId: fence,
            update: ChapterModelProviderUpdate(
                providerOperationId: "operation-1",
                providerStatus: "running"
            )
        )
        let completion = HostObservation.chapterModelCompleted(
            episodeId: episodeID,
            generation: 2,
            submissionFenceId: fence,
            completion: ChapterModelCompletionObservation(
                completion: #"{"chapters":[]}"#,
                provider: "openrouter",
                model: "model-a:canonical",
                promptTokens: 10,
                completionTokens: 4,
                cachedTokens: 0,
                reasoningTokens: 0,
                costMicrousd: 2,
                providerOperationId: "operation-1",
                providerStatus: "completed",
                providerGeneratedAt: nil
            )
        )
        let failed = HostObservation.chapterModelFailed(
            episodeId: episodeID,
            generation: 2,
            submissionFenceId: fence,
            code: .httpResponse(statusCode: 429),
            safeDetail: "rate limited",
            retryAfterMilliseconds: 30_000
        )
        XCTAssertNotEqual(provider, completion)
        XCTAssertNotEqual(completion, failed)
        XCTAssertEqual(
            HostObservation.coreWakeReached(reason: wakeReason),
            .coreWakeReached(reason: wakeReason)
        )
        XCTAssertEqual(
            HostObservationReceipt.persisted(requestId: requestID, terminal: true),
            .persisted(requestId: requestID, terminal: true)
        )

        let projection = ModelChapterWorkflowProjection(
            episodeId: episodeID,
            configuredModel: "openrouter:model-a",
            mode: .generate,
            sourceVersion: "model-chapters-v2",
            stage: .providerAccepted,
            workflowRevision: StateRevision(value: 3),
            generation: 2,
            attempt: 1,
            maxAttempts: 4,
            requestId: requestID,
            cancellationId: cancellationID,
            notBefore: nil,
            selectedArtifactId: nil,
            failure: nil,
            replanPending: false,
            mayHaveSubmitted: true,
            createdAt: UnixTimestampMilliseconds(value: 1),
            updatedAt: UnixTimestampMilliseconds(value: 2),
            allowedActions: ModelChapterWorkflowAllowedActions(
                canRetry: false,
                canCancel: false
            )
        )
        XCTAssertEqual(projection.stage, .providerAccepted)
    }

    func testGeneratedModelWorkflowCommandsRemainSemantic() {
        let episodeID = EpisodeId(high: 11, low: 12)
        let ensure = ApplicationCommand.ensureModelChapters(
            episodeId: episodeID,
            configuredModel: "ollama:llama3.2"
        )
        let retry = ApplicationCommand.retryModelChapters(
            episodeId: episodeID,
            configuredModel: "ollama:llama3.2",
            expectedWorkflowRevision: StateRevision(value: 4)
        )
        let cancel = ApplicationCommand.cancelModelChapters(
            episodeId: episodeID,
            expectedWorkflowRevision: StateRevision(value: 4)
        )
        XCTAssertNotEqual(ensure, retry)
        XCTAssertNotEqual(retry, cancel)
    }
}
