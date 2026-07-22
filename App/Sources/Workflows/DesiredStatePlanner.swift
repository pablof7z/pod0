import Foundation

struct DesiredStatePlanner: Sendable {
    struct Input: Sendable {
        let settings: Settings
        let scheduledTasks: [AgentScheduledTask]
        let now: Date

        init(
            settings: Settings,
            scheduledTasks: [AgentScheduledTask],
            now: Date
        ) {
            self.settings = settings
            self.scheduledTasks = scheduledTasks
            self.now = now
        }
    }

    func plan(_ input: Input) -> [DesiredJob] {
        var jobs: [DesiredJob] = []
        for task in input.scheduledTasks where task.nextRunAt <= input.now {
            let occurrence = Self.scheduledOccurrenceID(taskID: task.id, scheduledFor: task.nextRunAt)
            let payload = ScheduledRunPayload(
                taskID: task.id,
                scheduledFor: task.nextRunAt,
                prompt: task.prompt,
                modelID: input.settings.agentInitialModel,
                intervalSeconds: task.intervalSeconds
            )
            jobs.append(DesiredJob(
                idempotencyKey: occurrence,
                kind: .scheduledAgentRun,
                subjectID: task.id,
                inputVersion: occurrence,
                occurrenceID: occurrence,
                payload: try? Self.encoder.encode(payload),
                priority: 60,
                resourceClass: .scheduledAgent,
                maxAttempts: 12
            ))
        }
        return jobs.sorted { $0.idempotencyKey < $1.idempotencyKey }
    }

    static func audioVersion(_ episode: Episode) -> String {
        ArtifactRepository.version(parts: [
            episode.enclosureURL.absoluteString,
            episode.enclosureMimeType ?? "",
            String(episode.duration ?? 0),
        ])
    }

    static func scheduledOccurrenceID(taskID: UUID, scheduledFor: Date) -> String {
        "scheduled:\(taskID.uuidString):\(Int(scheduledFor.timeIntervalSince1970))"
    }

    private static let encoder: JSONEncoder = {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.sortedKeys]
        return encoder
    }()
}
