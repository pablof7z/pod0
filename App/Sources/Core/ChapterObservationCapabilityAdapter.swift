import Foundation
import Pod0Core

/// Temporary #110 model/agent shell. It owns only in-flight task handles;
/// Rust qualifies every successful observation and remains the durable owner.
@MainActor
final class ChapterObservationCapabilityAdapter {
    typealias Delivery = @MainActor (ChapterCapabilityResponse) -> Void

    private struct ActiveTask {
        let envelope: ChapterCapabilityRequestEnvelope
        let task: Task<Void, Never>
        let delivery: Delivery
    }

    private let modelTransport: any ChapterModelTransporting
    private let qualifier: any ChapterObservationQualifying
    private var activeTasks: [HostRequestId: ActiveTask] = [:]
    private var completedRequestIDs: Set<HostRequestId> = []
    private var completionOrder: [HostRequestId] = []

    init(
        modelTransport: any ChapterModelTransporting = LiveChapterModelTransport(),
        qualifier: any ChapterObservationQualifying = RustChapterObservationQualifier()
    ) {
        self.modelTransport = modelTransport
        self.qualifier = qualifier
    }

    func execute(
        _ envelope: ChapterCapabilityRequestEnvelope,
        delivery: @escaping Delivery
    ) {
        guard !isKnown(envelope.requestID) else { return }
        guard let limits = qualifier.limits() else {
            finish(envelope, outcome: .failed(.coreUnavailable), delivery: delivery)
            return
        }
        if let failure = Self.preflight(envelope.request, limits: limits) {
            finish(envelope, outcome: .failed(failure), delivery: delivery)
            return
        }

        let task = Task { @MainActor [weak self] in
            guard let self else { return }
            let outcome = await perform(envelope.request, limits: limits)
            guard activeTasks.removeValue(forKey: envelope.requestID) != nil else { return }
            finish(envelope, outcome: outcome, delivery: delivery)
        }
        activeTasks[envelope.requestID] = ActiveTask(
            envelope: envelope,
            task: task,
            delivery: delivery
        )
    }

    /// Async job-facing adapter over the same bounded capability lifecycle.
    /// The native shell owns only the in-flight task; Rust still qualifies the
    /// raw observation and decides whether it can become a domain artifact.
    func execute(
        _ envelope: ChapterCapabilityRequestEnvelope
    ) async -> ChapterCapabilityResponse {
        await withTaskCancellationHandler {
            await withCheckedContinuation { continuation in
                execute(envelope) { response in
                    continuation.resume(returning: response)
                }
            }
        } onCancel: {
            Task { @MainActor [weak self] in
                self?.cancel(cancellationID: envelope.cancellationID)
            }
        }
    }

    func cancel(cancellationID: CancellationId) {
        let requestIDs = activeTasks.compactMap { requestID, active in
            active.envelope.cancellationID == cancellationID ? requestID : nil
        }
        for requestID in requestIDs {
            guard let active = activeTasks.removeValue(forKey: requestID) else { continue }
            active.task.cancel()
            finish(
                active.envelope,
                outcome: .failed(.cancelled),
                delivery: active.delivery
            )
        }
    }

    func shutdown() {
        for requestID in Array(activeTasks.keys) {
            guard let active = activeTasks.removeValue(forKey: requestID) else { continue }
            active.task.cancel()
            finish(
                active.envelope,
                outcome: .failed(.cancelled),
                delivery: active.delivery
            )
        }
    }

    private func perform(
        _ request: ChapterCapabilityRequest,
        limits: ChapterObservationLimits
    ) async -> ChapterCapabilityOutcome {
        if Task.isCancelled { return .failed(.cancelled) }
        switch request {
        case .model(let value):
            return await performModel(value, limits: limits)
        case .agent(let value):
            return performAgent(value)
        }
    }

    private func performModel(
        _ request: ModelChapterCapabilityRequest,
        limits: ChapterObservationLimits
    ) async -> ChapterCapabilityOutcome {
        let result = await modelTransport.execute(
            request,
            maximumCompletionBytes: limits.modelCompletionBytes
        )
        if Task.isCancelled { return .failed(.cancelled) }
        switch result {
        case .failure(let failure):
            return .failed(failure)
        case .success(let response):
            return qualifyModel(response, request: request, limits: limits)
        }
    }

    private func performAgent(
        _ request: AgentChapterCapabilityRequest
    ) -> ChapterCapabilityOutcome {
        let observation = AgentComposedChapterObservation(
            episodeId: request.episodeID,
            podcastId: request.podcastID,
            compositionRevision: request.compositionRevision,
            policyVersion: request.policyVersion,
            provider: request.provider,
            model: request.model,
            sourcePayloadDigest: request.sourcePayloadDigest,
            generatedAt: request.generatedAt,
            durationMilliseconds: request.durationMilliseconds,
            items: request.items
        )
        return qualify(
            .agent(observation),
            evidence: .agent(ChapterAgentEvidence(
                sourcePayloadDigest: request.sourcePayloadDigest,
                orderedItemCount: UInt32(request.items.count)
            ))
        )
    }

    func qualify(
        _ observation: ChapterRawObservation,
        evidence: ChapterCapabilityEvidence
    ) -> ChapterCapabilityOutcome {
        guard let qualification = qualifier.qualify(observation) else {
            return .failed(.coreUnavailable)
        }
        return .observed(
            observation: observation,
            evidence: evidence,
            qualification: qualification
        )
    }

    private func finish(
        _ envelope: ChapterCapabilityRequestEnvelope,
        outcome: ChapterCapabilityOutcome,
        delivery: Delivery
    ) {
        rememberCompletion(envelope.requestID)
        delivery(ChapterCapabilityResponse(
            requestID: envelope.requestID,
            cancellationID: envelope.cancellationID,
            outcome: outcome
        ))
    }

    private func isKnown(_ requestID: HostRequestId) -> Bool {
        activeTasks[requestID] != nil || completedRequestIDs.contains(requestID)
    }

    private func rememberCompletion(_ requestID: HostRequestId) {
        guard completedRequestIDs.insert(requestID).inserted else { return }
        completionOrder.append(requestID)
        if completionOrder.count > 256 {
            completedRequestIDs.remove(completionOrder.removeFirst())
        }
    }
}
