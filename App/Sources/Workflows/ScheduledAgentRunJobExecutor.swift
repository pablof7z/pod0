import Foundation

@MainActor
final class ScheduledAgentRunJobExecutor: JobExecutor {
    private let store: AppStateStore
    private let artifacts: ArtifactRepository
    private let history: ChatHistoryStore
    private let deps: @MainActor @Sendable () -> PodcastAgentToolDeps?

    init(
        store: AppStateStore,
        artifacts: ArtifactRepository,
        history: ChatHistoryStore = .shared,
        deps: @escaping @MainActor @Sendable () -> PodcastAgentToolDeps?
    ) {
        self.store = store
        self.artifacts = artifacts
        self.history = history
        self.deps = deps
    }

    func run(_ context: JobAttemptContext) async throws -> JobOutcome {
        let payload = try decodePayload(context.job)
        guard store.scheduledTasks.contains(where: { $0.id == payload.taskID }) else {
            return .obsolete
        }
        if try artifacts.current(
            kind: .scheduledOutput,
            subjectID: payload.taskID
        )?.outputVersion == context.job.occurrenceID {
            return .succeeded(outputVersion: context.job.occurrenceID)
        }
        if let occurrenceID = context.job.occurrenceID,
           history.conversation(occurrenceID: occurrenceID)?.hasCompletedScheduledOutput == true {
            return .succeeded(outputVersion: occurrenceID)
        }
        let reference = LLMModelReference(storedID: payload.modelID)
        guard LLMProviderCredentialResolver.hasAPIKey(for: reference.provider) else {
            return .blocked(reason: JobFailure(
                classification: .missingCredential,
                message: "No agent API key is configured."
            ))
        }
        let session = AgentChatSession(
            store: store,
            podcastDeps: deps(),
            history: history,
            resumeWindow: 0,
            drainPendingContext: false,
            scheduledOccurrenceID: context.job.occurrenceID
        )
        session.isScheduledTask = true
        if context.job.occurrenceID.flatMap({ history.conversation(occurrenceID: $0) }) != nil {
            await session.resumeScheduledRun(fallbackPrompt: payload.prompt)
        } else {
            await session.send(payload.prompt, source: .scheduledTask)
        }
        if case .failed(let failure) = session.phase {
            throw JobFailure(
                classification: failure.code.jobErrorClass,
                message: failure.diagnosticSummary
            )
        }
        guard let occurrenceID = context.job.occurrenceID,
              history.conversation(occurrenceID: occurrenceID)?.hasCompletedScheduledOutput == true
        else {
            throw JobFailure(
                classification: .transient,
                message: "Scheduled agent run did not produce a completed assistant response."
            )
        }
        return .succeeded(outputVersion: occurrenceID)
    }

    private func decodePayload(_ job: WorkJob) throws -> ScheduledRunPayload {
        guard let payload = job.payload else {
            throw JobFailure(
                classification: .invalidInput,
                message: "Missing scheduled-run payload."
            )
        }
        do { return try Self.decoder.decode(ScheduledRunPayload.self, from: payload) }
        catch {
            throw JobFailure(
                classification: .invalidInput,
                message: "Scheduled-run payload is invalid."
            )
        }
    }

    private static let decoder: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return decoder
    }()
}
