import Foundation
import Pod0Core

// MARK: - ChatMessage

/// A single message in the agent chat transcript.
struct ChatMessage: Identifiable, Equatable {
    enum Role: Equatable {
        case user
        case assistant
        case toolBatch(batchID: UUID, count: Int)
        case error
        case skillActivated(skillID: String, displayName: String)
    }

    let id: UUID
    let role: Role
    let text: String
    let timestamp: Date
    let recallEvidence: [RecallEvidenceProjection]

    init(
        id: UUID = UUID(),
        role: Role,
        text: String,
        timestamp: Date = Date(),
        recallEvidence: [RecallEvidenceProjection] = []
    ) {
        self.id = id
        self.role = role
        self.text = text
        self.timestamp = timestamp
        self.recallEvidence = recallEvidence
    }
}
