import Foundation

/// Temporary decode-only shape for current development migration so older AppState
/// metadata can be retired explicitly after shared Agent authority is ready.
/// No product path may create or render these records.
enum LegacyAgentActivityKind: Codable, Equatable, Sendable {
    case noteCreated(noteID: UUID)
    case memoryRecorded(memoryID: UUID)
}

struct LegacyAgentActivityEntry: Codable, Equatable, Sendable {
    let id: UUID
    let batchID: UUID
    let timestamp: Date
    let kind: LegacyAgentActivityKind
    let summary: String
    let undone: Bool
}
