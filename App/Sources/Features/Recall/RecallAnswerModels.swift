import Foundation

struct RecallAnswer: Identifiable, Codable, Equatable, Sendable {
    enum Status: String, Codable, Equatable, Sendable {
        case ready
        case indexing
        case transcriptMissing
        case indexMissing
        case noEvidence
        case indexUnavailable
        case providerUnavailable
        case corruptArtifact
        case interrupted
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
    let spanID: String
    let episodeID: UUID
    let podcastID: UUID
    let episodeTitle: String
    let podcastTitle: String
    let generationID: String
    let transcriptVersionID: String
    let transcriptContentDigest: String
    let firstSegmentID: String
    let lastSegmentID: String
    let startSegmentOrdinal: UInt32
    let endSegmentOrdinalExclusive: UInt32
    let startMilliseconds: UInt64
    let endMilliseconds: UInt64
    let excerpt: String
    let speakerID: String?
    let provenance: RecallEvidenceProvenance
    let score: RecallEvidenceScore

    var id: String { spanID }
}

struct RecallEvidenceProvenance: Codable, Equatable, Sendable {
    let source: String
    let provider: String?
    let sourcePayloadDigest: String
}

struct RecallEvidenceScore: Codable, Equatable, Sendable {
    let vectorRRFUnits: UInt64
    let lexicalRRFUnits: UInt64
    let totalRRFUnits: UInt64
    let baseRank: UInt16
    let rerankRank: UInt16?
}

struct RecallEvidenceMetadata: Equatable, Sendable {
    let episodeTitle: String
    let podcastTitle: String
}
