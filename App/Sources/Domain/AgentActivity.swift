import Foundation

// MARK: - Agent Activity
// One row per agent-driven mutation, capturing just enough to render a
// human-readable summary and undo the effect by flipping a soft-delete.
// Grouped by `batchID` (one batch per agent run).

enum AgentActivityKind: Codable, Hashable, Sendable {
    case noteCreated(noteID: UUID)
    case memoryRecorded(memoryID: UUID)
}

struct AgentActivityEntry: Codable, Identifiable, Hashable, Sendable {
    var id: UUID
    var batchID: UUID
    var timestamp: Date
    var kind: AgentActivityKind
    var summary: String
    var undone: Bool

    init(batchID: UUID, kind: AgentActivityKind, summary: String) {
        self.id = UUID()
        self.batchID = batchID
        self.timestamp = Date()
        self.kind = kind
        self.summary = summary
        self.undone = false
    }
}
