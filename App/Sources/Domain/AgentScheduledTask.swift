import Foundation

struct AgentScheduledTask: Codable, Identifiable, Sendable {
    let id: UUID
    var label: String
    var prompt: String
    var intervalSeconds: TimeInterval
    let createdAt: Date
    var lastRunAt: Date?
    var nextRunAt: Date

    var isDue: Bool { Date() >= nextRunAt }
}
