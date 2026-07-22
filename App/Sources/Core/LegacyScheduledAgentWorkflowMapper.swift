import Foundation
import Pod0Core

enum LegacyScheduledAgentWorkflowMappingError: Error, Equatable {
    case duplicateTask(UUID)
    case duplicateOccurrence(String)
    case duplicateConversation(String)
    case invalidTask(UUID)
    case invalidJob(UUID)
    case attemptOutOfRange(UUID)
}

enum LegacyScheduledAgentWorkflowMapper {
    private static let maximumAttempt: Int = 12

    static func map(
        _ backup: LegacyScheduledAgentWorkflowBackup
    ) throws -> (tasks: [LegacyScheduledAgentTaskInput], occurrences: [LegacyScheduledAgentOccurrenceInput]) {
        var legacyTasks: [UUID: AgentScheduledTask] = [:]
        for task in backup.tasks {
            guard legacyTasks.updateValue(task, forKey: task.id) == nil else {
                throw LegacyScheduledAgentWorkflowMappingError.duplicateTask(task.id)
            }
            guard intervalMilliseconds(task.intervalSeconds) != nil,
                  timestamp(task.createdAt) != nil,
                  timestamp(task.nextRunAt) != nil,
                  task.lastRunAt.map({ timestamp($0) != nil }) ?? true
            else { throw LegacyScheduledAgentWorkflowMappingError.invalidTask(task.id) }
        }
        let conversations = try conversationsByOccurrence(backup.conversations)
        var occurrenceIDs = Set<String>()
        var occurrences: [LegacyScheduledAgentOccurrenceInput] = []

        for job in backup.jobs {
            guard let payload = decode(job), payload.taskID == job.subjectID else {
                throw LegacyScheduledAgentWorkflowMappingError.invalidJob(job.id)
            }
            guard legacyTasks[payload.taskID] != nil else { continue }
            let occurrenceID = DesiredStatePlanner.scheduledOccurrenceID(
                taskID: payload.taskID,
                scheduledFor: payload.scheduledFor
            )
            guard job.occurrenceID == occurrenceID,
                  job.idempotencyKey == occurrenceID,
                  job.inputVersion == occurrenceID,
                  let scheduledFor = timestamp(payload.scheduledFor),
                  let createdAt = timestamp(job.createdAt),
                  let updatedAt = timestamp(job.updatedAt),
                  updatedAt.value >= createdAt.value,
                  !payload.prompt.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
                  !payload.modelID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            else {
                throw LegacyScheduledAgentWorkflowMappingError.invalidJob(job.id)
            }
            guard occurrenceIDs.insert(occurrenceID).inserted else {
                throw LegacyScheduledAgentWorkflowMappingError.duplicateOccurrence(occurrenceID)
            }
            let disposition = try disposition(
                job,
                output: conversations[occurrenceID]?.completedScheduledOutputText
            )
            occurrences.append(LegacyScheduledAgentOccurrenceInput(
                taskId: ScheduledTaskId(uuid: payload.taskID),
                scheduledFor: scheduledFor,
                createdAt: createdAt,
                prompt: payload.prompt,
                modelReference: payload.modelID,
                updatedAt: updatedAt,
                disposition: disposition
            ))
            if case .succeeded = disposition,
               var task = legacyTasks[payload.taskID],
               UnixTimestampMilliseconds(date: task.nextRunAt) == scheduledFor {
                task.lastRunAt = job.updatedAt
                task.nextRunAt = job.updatedAt.addingTimeInterval(task.intervalSeconds)
                legacyTasks[payload.taskID] = task
            }
        }

        let taskInputs = try legacyTasks.values
            .sorted { $0.id.uuidString < $1.id.uuidString }
            .map { task in
                guard let interval = intervalMilliseconds(task.intervalSeconds),
                      let createdAt = timestamp(task.createdAt),
                      let nextRunAt = timestamp(task.nextRunAt)
                else { throw LegacyScheduledAgentWorkflowMappingError.invalidTask(task.id) }
                return LegacyScheduledAgentTaskInput(
                    taskId: ScheduledTaskId(uuid: task.id),
                    label: task.label,
                    prompt: task.prompt,
                    modelReference: backup.defaultModelReference,
                    intervalMilliseconds: interval,
                    createdAt: createdAt,
                    lastRunAt: task.lastRunAt.flatMap { timestamp($0) },
                    nextRunAt: nextRunAt
                )
            }
        return (taskInputs, occurrences.sorted { lhs, rhs in
            if lhs.taskId != rhs.taskId {
                if lhs.taskId.high != rhs.taskId.high {
                    return lhs.taskId.high < rhs.taskId.high
                }
                return lhs.taskId.low < rhs.taskId.low
            }
            return lhs.scheduledFor.value < rhs.scheduledFor.value
        })
    }
}

private extension LegacyScheduledAgentWorkflowMapper {
    static func conversationsByOccurrence(
        _ conversations: [ChatConversation]
    ) throws -> [String: ChatConversation] {
        var result: [String: ChatConversation] = [:]
        for conversation in conversations where conversation.isScheduledTask {
            guard let occurrenceID = conversation.occurrenceID else { continue }
            guard result.updateValue(conversation, forKey: occurrenceID) == nil else {
                throw LegacyScheduledAgentWorkflowMappingError.duplicateConversation(occurrenceID)
            }
        }
        return result
    }

    static func decode(_ job: WorkJob) -> ScheduledRunPayload? {
        guard job.kind == .scheduledAgentRun, let data = job.payload else { return nil }
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return try? decoder.decode(ScheduledRunPayload.self, from: data)
    }

    static func disposition(
        _ job: WorkJob,
        output: String?
    ) throws -> LegacyScheduledAgentOccurrenceDisposition {
        switch job.state {
        case .pending where job.attempt == 0:
            return .pending
        case .pending:
            return .retryScheduled(
                attempt: try attempt(job), notBefore: UnixTimestampMilliseconds(date: job.notBefore),
                failureCode: .unexpected, safeDetail: "Legacy pending attempt will restart"
            )
        case .leased, .running:
            return .ambiguous(
                attempt: try attempt(job),
                safeDetail: "Legacy host operation ownership is ambiguous after restart"
            )
        case .retryScheduled:
            return .retryScheduled(
                attempt: try attempt(job), notBefore: UnixTimestampMilliseconds(date: job.notBefore),
                failureCode: failureCode(job.lastErrorClass),
                safeDetail: bounded(job.lastErrorMessage, bytes: 1_024)
            )
        case .blocked:
            return .blocked(
                attempt: try attempt(job), failureCode: failureCode(job.lastErrorClass),
                safeDetail: bounded(job.lastErrorMessage, bytes: 1_024), retryable: true
            )
        case .failedPermanent:
            return .failedPermanent(
                attempt: try attempt(job), failureCode: failureCode(job.lastErrorClass),
                safeDetail: bounded(job.lastErrorMessage, bytes: 1_024)
            )
        case .cancelled:
            return .cancelled(attempt: try attempt(job))
        case .obsolete:
            return .obsolete(attempt: try attempt(job))
        case .succeeded:
            guard let output, !output.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
                return .blocked(
                    attempt: try attempt(job), failureCode: .invalidOutput,
                    safeDetail: "Legacy completion output is unavailable", retryable: true
                )
            }
            return .succeeded(
                attempt: try attempt(job),
                outputExcerpt: bounded(output, bytes: 16 * 1_024) ?? ""
            )
        }
    }

    static func attempt(_ job: WorkJob) throws -> UInt16 {
        guard job.attempt > 0, job.attempt <= maximumAttempt,
              let value = UInt16(exactly: job.attempt) else {
            throw LegacyScheduledAgentWorkflowMappingError.attemptOutOfRange(job.id)
        }
        return value
    }

    static func failureCode(_ value: JobErrorClass?) -> ScheduledAgentFailureCode {
        switch value {
        case .missingCredential: .missingCredential
        case .offline: .offline
        case .network: .network
        case .rateLimited: .rateLimited
        case .missingDependency: .providerUnavailable
        case .unsafeToRetry: .unsafeToRetry
        case .unsupportedFormat, .corruptArtifact, .invalidInput: .invalidOutput
        case .cancelled: .cancelled
        case .transient, .unexpected, nil: .unexpected
        }
    }

    static func intervalMilliseconds(_ seconds: TimeInterval) -> UInt64? {
        guard seconds.isFinite, seconds > 0,
              seconds <= Double(UInt64.max) / 1_000 else { return nil }
        return UInt64((seconds * 1_000).rounded())
    }

    static func timestamp(_ date: Date) -> UnixTimestampMilliseconds? {
        let value = UnixTimestampMilliseconds(date: date)
        return value.value >= 0 ? value : nil
    }

    static func bounded(_ value: String?, bytes limit: Int) -> String? {
        guard let value else { return nil }
        var byteCount = 0
        let scalars = value.unicodeScalars.prefix { scalar in
            let next = byteCount + scalar.utf8.count
            guard next <= limit else { return false }
            byteCount = next
            return true
        }
        return String(scalars)
    }
}
