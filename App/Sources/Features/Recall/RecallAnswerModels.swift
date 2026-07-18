import Foundation

struct RecallAnswer: Identifiable, Codable, Equatable, Sendable {
    enum Status: String, Codable, Equatable, Sendable {
        case ready
        case indexing
        case transcriptMissing
        case noEvidence
        case unavailable
        case cancelled
    }

    let id: UUID
    let text: String
    let evidence: [RecallEvidence]
    let status: Status

    init(
        id: UUID = UUID(),
        text: String,
        evidence: [RecallEvidence] = [],
        status: Status
    ) {
        self.id = id
        self.text = text
        self.evidence = evidence
        self.status = status
    }
}

struct RecallEvidence: Identifiable, Codable, Equatable, Sendable {
    let chunkID: UUID
    let episodeID: UUID
    let podcastID: UUID
    let episodeTitle: String
    let podcastTitle: String
    let artifactVersion: String
    let startMilliseconds: Int64
    let endMilliseconds: Int64
    let excerpt: String
    let provenance: String

    var id: UUID { chunkID }
}

struct RecallEvidenceMetadata: Equatable, Sendable {
    let episodeTitle: String
    let podcastTitle: String
}
