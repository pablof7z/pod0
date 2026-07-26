import Foundation
import Pod0Core

@MainActor
protocol SharedRecallSearching: AnyObject {
    func recall(
        query: String,
        scope: RecallScope,
        limit: UInt16
    ) async -> RecallResultProjection
}

@MainActor
final class SharedRecallWaiter {
    let subscriber: SharedRecallSubscriber
    let continuation: CheckedContinuation<RecallResultProjection, Never>
    var subscriptionID: SubscriptionId?
    var cancellationRequested = false
    var cancellationDispatched = false

    init(
        subscriber: SharedRecallSubscriber,
        continuation: CheckedContinuation<RecallResultProjection, Never>
    ) {
        self.subscriber = subscriber
        self.continuation = continuation
    }
}

extension SharedLibraryClient: SharedRecallSearching {
    func recall(
        query: String,
        scope: RecallScope,
        limit: UInt16
    ) async -> RecallResultProjection {
        let queryID = RecallQueryId(uuid: UUID())
        let commandID = CommandId(uuid: UUID())
        let cancellationID = CancellationId(uuid: UUID())
        return await withTaskCancellationHandler {
            await withCheckedContinuation { continuation in
                let subscriber = SharedRecallSubscriber { [weak self] envelope in
                    Task { @MainActor [weak self] in
                        self?.receiveRecall(queryID: queryID, envelope: envelope)
                    }
                }
                let command = CommandEnvelope(
                    commandId: commandID,
                    cancellationId: cancellationID,
                    expectedRevision: nil,
                    command: .recallQuery(query: RecallQuery(
                        queryId: queryID,
                        text: query,
                        scope: scope,
                        limit: limit
                    ))
                )
                let request = ProjectionRequest(
                        scope: .recall(queryId: queryID),
                        offset: 0,
                        maxItems: limit
                )
                let waiter = SharedRecallWaiter(
                    subscriber: subscriber,
                    continuation: continuation
                )
                recallWaiters[queryID] = waiter
                let executor = commandExecutor
                let facade = facade
                Task { @MainActor [weak self] in
                    let subscriptionID = await executor.dispatchThenSubscribe(
                        request: request,
                        subscriber: subscriber,
                        envelope: command,
                        to: facade
                    )
                    guard let self,
                          let active = recallWaiters[queryID],
                          active === waiter
                    else {
                        await executor.unsubscribe(subscriptionID, from: facade)
                        return
                    }
                    active.subscriptionID = subscriptionID
                    if active.cancellationRequested {
                        sendRecallCancellation(
                            queryID: queryID,
                            cancellationID: cancellationID
                        )
                    }
                    executePendingHostRequests()
                }
                if Task.isCancelled {
                    cancelRecall(queryID: queryID, cancellationID: cancellationID)
                }
            }
        } onCancel: {
            Task { @MainActor [weak self] in
                self?.cancelRecall(queryID: queryID, cancellationID: cancellationID)
            }
        }
    }

    private func receiveRecall(queryID: RecallQueryId, envelope: ProjectionEnvelope) {
        guard case .recall(let projection) = envelope.projection,
              projection.queryId == queryID,
              recallWaiters[queryID] != nil else { return }
        guard projection.stage.isTerminal else {
            executePendingHostRequests()
            return
        }
        finishRecall(queryID: queryID, projection: projection)
    }

    private func cancelRecall(queryID: RecallQueryId, cancellationID: CancellationId) {
        guard let waiter = recallWaiters[queryID] else { return }
        waiter.cancellationRequested = true
        guard waiter.subscriptionID != nil else { return }
        sendRecallCancellation(queryID: queryID, cancellationID: cancellationID)
    }

    private func sendRecallCancellation(
        queryID: RecallQueryId,
        cancellationID: CancellationId
    ) {
        guard let waiter = recallWaiters[queryID],
              !waiter.cancellationDispatched else { return }
        waiter.cancellationDispatched = true
        dispatchCoreCommand(.cancelOperation(cancellationId: cancellationID))
        cancelPendingHostRequests(cancellationID: cancellationID)
    }

    private func finishRecall(queryID: RecallQueryId, projection: RecallResultProjection) {
        guard let waiter = recallWaiters.removeValue(forKey: queryID) else { return }
        if let subscriptionID = waiter.subscriptionID {
            let executor = commandExecutor
            let facade = facade
            Task {
                await executor.unsubscribe(subscriptionID, from: facade)
            }
        }
        waiter.continuation.resume(returning: projection)
    }

    func cancelAllRecallWaiters() {
        for queryID in Array(recallWaiters.keys) {
            finishRecall(queryID: queryID, projection: RecallResultProjection(
                queryId: queryID,
                stage: .interrupted,
                evidence: [],
                failure: nil,
                operation: nil
            ))
        }
    }
}

extension RecallStage {
    var isTerminal: Bool {
        switch self {
        case .queued, .running:
            false
        case .ready, .noEvidence, .transcriptMissing, .indexMissing, .indexing,
             .indexUnavailable, .providerUnavailable, .corruptArtifact, .interrupted,
             .cancelled, .failed, .unsupported:
            true
        }
    }

    var stableName: String {
        switch self {
        case .queued: "queued"
        case .running: "running"
        case .ready: "ready"
        case .noEvidence: "no_evidence"
        case .transcriptMissing: "transcript_missing"
        case .indexMissing: "index_missing"
        case .indexing: "indexing"
        case .indexUnavailable: "index_unavailable"
        case .providerUnavailable: "provider_unavailable"
        case .corruptArtifact: "corrupt_artifact"
        case .interrupted: "interrupted"
        case .cancelled: "cancelled"
        case .failed: "failed"
        case .unsupported(let wireCode): "unsupported:\(wireCode)"
        }
    }
}

extension RecallResultProjection {
    static func interrupted() -> RecallResultProjection {
        RecallResultProjection(
            queryId: RecallQueryId(uuid: UUID()),
            stage: .interrupted,
            evidence: [],
            failure: nil,
            operation: nil
        )
    }
}

final class SharedRecallSubscriber: ProjectionSubscriber, @unchecked Sendable {
    private let delivery: @Sendable (ProjectionEnvelope) -> Void

    init(delivery: @escaping @Sendable (ProjectionEnvelope) -> Void) {
        self.delivery = delivery
    }

    func receive(projection: ProjectionEnvelope) {
        delivery(projection)
    }
}
