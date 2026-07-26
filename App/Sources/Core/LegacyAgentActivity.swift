import Foundation

/// Decode-only shape retained for one support window so older AppState
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
