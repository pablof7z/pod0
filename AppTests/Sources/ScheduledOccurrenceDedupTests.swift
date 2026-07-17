import Foundation
import XCTest
@testable import Podcastr

@MainActor
final class ScheduledOccurrenceDedupTests: XCTestCase {
    func testVisibleConversationIsUniqueAndResumableByOccurrence() {
        let fileURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("\(UUID().uuidString).json")
        defer { try? FileManager.default.removeItem(at: fileURL) }
        let history = ChatHistoryStore(fileURL: fileURL)
        let occurrence = "scheduled:task:1000"
        let first = ChatConversation(
            id: OccurrenceIdentity.uuid(for: occurrence),
            title: "First",
            messages: [.init(role: .user, text: "Run")],
            isScheduledTask: true,
            occurrenceID: occurrence
        )
        history.upsert(first)
        var resumed = first
        resumed.messages.append(.init(role: .assistant, text: "Finished"))
        resumed.updatedAt = Date().addingTimeInterval(1)
        history.upsert(resumed)

        XCTAssertEqual(history.conversations.count, 1)
        XCTAssertEqual(history.conversation(occurrenceID: occurrence)?.messages.count, 2)
        XCTAssertEqual(
            history.conversation(occurrenceID: occurrence)?.id,
            OccurrenceIdentity.uuid(for: occurrence)
        )
    }

    func testOnlySucceededExactOccurrenceAdvancesScheduleOnce() {
        let made = AppStateTestSupport.makeIsolatedStore()
        defer { AppStateTestSupport.disposeIsolatedStore(at: made.fileURL) }
        let due = Date(timeIntervalSince1970: 10_000)
        let task = AgentScheduledTask(
            id: UUID(), label: "Brief", prompt: "Run", intervalSeconds: 3_600,
            createdAt: due.addingTimeInterval(-100), lastRunAt: nil, nextRunAt: due
        )
        made.store.mutateState { $0.agentScheduledTasks = [task] }
        let key = DesiredStatePlanner.scheduledOccurrenceID(
            taskID: task.id, scheduledFor: due
        )
        let pending = workJob(key: key, taskID: task.id, state: .failedPermanent)

        XCTAssertEqual(
            made.store.advanceCompletedScheduledOccurrences(from: [pending], now: due),
            0
        )
        XCTAssertEqual(made.store.scheduledTasks[0].nextRunAt, due)

        let succeeded = workJob(key: key, taskID: task.id, state: .succeeded)
        XCTAssertEqual(
            made.store.advanceCompletedScheduledOccurrences(from: [succeeded], now: due),
            1
        )
        XCTAssertEqual(made.store.scheduledTasks[0].lastRunAt, due)
        XCTAssertEqual(made.store.scheduledTasks[0].nextRunAt, due.addingTimeInterval(3_600))
        XCTAssertEqual(
            made.store.advanceCompletedScheduledOccurrences(from: [succeeded], now: due),
            0
        )
    }

    private func workJob(
        key: String,
        taskID: UUID,
        state: WorkJobState
    ) -> WorkJob {
        WorkJob(
            id: UUID(), idempotencyKey: key, kind: .scheduledAgentRun,
            subjectID: taskID, inputVersion: key, occurrenceID: key,
            payloadVersion: 1, payload: nil, state: state, priority: 0,
            resourceClass: .scheduledAgent, attempt: 1, maxAttempts: 8,
            notBefore: Date(), leaseToken: nil, leaseOwner: nil,
            leaseExpiresAt: nil, externalProvider: nil,
            externalOperationID: nil, externalOperationState: nil,
            outputVersion: state == .succeeded ? key : nil,
            lastErrorClass: nil, lastErrorMessage: nil,
            createdAt: Date(), updatedAt: Date()
        )
    }
}
