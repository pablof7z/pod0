import Foundation
@testable import Podcastr

enum LegacyScheduledAgentWorkflowTestSupport {
    static let taskID = UUID(uuidString: "91000000-0000-0000-0000-000000000001")!
    static let baseDate = Date(timeIntervalSince1970: 1_700_000_000)

    static func task(nextRunAt: Date = baseDate) -> AgentScheduledTask {
        AgentScheduledTask(
            id: taskID,
            label: "Daily briefing",
            prompt: "Summarize my saved evidence",
            intervalSeconds: 3_600,
            createdAt: baseDate.addingTimeInterval(-3_600),
            lastRunAt: nil,
            nextRunAt: nextRunAt
        )
    }

    static func job(
        scheduledFor: Date,
        state: WorkJobState,
        attempt: Int,
        payloadOverride: Data? = nil,
        usePayloadOverride: Bool = false
    ) -> WorkJob {
        let occurrence = DesiredStatePlanner.scheduledOccurrenceID(
            taskID: taskID,
            scheduledFor: scheduledFor
        )
        let payload = ScheduledRunPayload(
            taskID: taskID,
            scheduledFor: scheduledFor,
            prompt: "Summarize my saved evidence",
            modelID: "openrouter:test/model",
            intervalSeconds: 3_600
        )
        return WorkJob(
            id: OccurrenceIdentity.uuid(for: occurrence),
            idempotencyKey: occurrence,
            kind: .scheduledAgentRun,
            subjectID: taskID,
            inputVersion: occurrence,
            occurrenceID: occurrence,
            payloadVersion: 1,
            payload: usePayloadOverride ? payloadOverride : try! encoder.encode(payload),
            state: state,
            priority: 60,
            resourceClass: .scheduledAgent,
            attempt: attempt,
            maxAttempts: 12,
            notBefore: scheduledFor.addingTimeInterval(60),
            leaseToken: state == .leased || state == .running ? UUID() : nil,
            leaseOwner: state == .leased || state == .running ? "legacy-owner" : nil,
            leaseExpiresAt: state == .leased || state == .running
                ? scheduledFor.addingTimeInterval(300) : nil,
            externalProvider: nil,
            externalOperationID: nil,
            externalOperationState: nil,
            outputVersion: state == .succeeded ? occurrence : nil,
            lastErrorClass: failureClass(state),
            lastErrorMessage: failureClass(state).map { _ in "Legacy failure" },
            createdAt: scheduledFor,
            updatedAt: scheduledFor.addingTimeInterval(30)
        )
    }

    static func conversation(scheduledFor: Date, output: String) -> ChatConversation {
        let occurrence = DesiredStatePlanner.scheduledOccurrenceID(
            taskID: taskID,
            scheduledFor: scheduledFor
        )
        return ChatConversation(
            id: OccurrenceIdentity.uuid(for: occurrence),
            messages: [
                .init(role: .user, text: "Summarize my saved evidence"),
                .init(role: .assistant, text: output),
            ],
            isScheduledTask: true,
            occurrenceID: occurrence,
            createdAt: scheduledFor,
            updatedAt: scheduledFor.addingTimeInterval(30)
        )
    }

    static func backup(
        jobs: [WorkJob],
        conversations: [ChatConversation] = []
    ) -> LegacyScheduledAgentWorkflowBackup {
        LegacyScheduledAgentWorkflowBackup(
            formatVersion: 1,
            persistenceGeneration: 7,
            defaultModelReference: "openrouter:test/model",
            tasks: [task()],
            jobs: jobs.sorted { $0.id.uuidString < $1.id.uuidString },
            artifacts: [],
            conversations: conversations.sorted { $0.id.uuidString < $1.id.uuidString }
        )
    }

    static let encoder: JSONEncoder = {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.sortedKeys]
        return encoder
    }()

    private static func failureClass(_ state: WorkJobState) -> JobErrorClass? {
        switch state {
        case .blocked: .missingCredential
        case .retryScheduled: .network
        case .failedPermanent: .invalidInput
        default: nil
        }
    }
}
