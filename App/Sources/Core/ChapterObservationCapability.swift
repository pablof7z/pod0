import Foundation
import Pod0Core

// Temporary request orchestration for #102. Issue #100 deletes this Swift
// workflow shell once Rust emits the equivalent typed host requests; the raw
// URLSession and credential capabilities remain native by design.

struct ChapterCapabilityRequestEnvelope: Equatable, Sendable {
    let requestID: HostRequestId
    let cancellationID: CancellationId
    let request: ChapterCapabilityRequest
}

enum ChapterCapabilityRequest: Equatable, Sendable {
    case publisher(PublisherChapterCapabilityRequest)
    case model(ModelChapterCapabilityRequest)
    case agent(AgentChapterCapabilityRequest)
}

struct PublisherChapterCapabilityRequest: Equatable, Sendable {
    let episodeID: EpisodeId
    let podcastID: PodcastId
    let sourceURL: String
    let generatedAt: UnixTimestampMilliseconds
    let durationMilliseconds: UInt64?
    let deadlineAt: UnixTimestampMilliseconds?
}

struct ModelChapterCapabilityRequest: Equatable, Sendable {
    let episodeID: EpisodeId
    let podcastID: PodcastId
    let formatVersion: UInt32
    let requestedTranscriptVersionID: TranscriptVersionId
    let requestedTranscriptContentDigest: ContentDigest
    let selectedTranscriptVersionID: TranscriptVersionId
    let selectedTranscriptContentDigest: ContentDigest
    let policyVersion: UInt32
    let provider: String
    let model: String
    let systemPrompt: String
    let userPrompt: String
    let generatedAt: UnixTimestampMilliseconds
    let durationMilliseconds: UInt64?
    let mode: ChapterModelObservationMode
}

struct AgentChapterCapabilityRequest: Equatable, Sendable {
    let episodeID: EpisodeId
    let podcastID: PodcastId
    let compositionRevision: String
    let policyVersion: UInt32
    let provider: String?
    let model: String?
    let sourcePayloadDigest: ContentDigest
    let generatedAt: UnixTimestampMilliseconds
    let durationMilliseconds: UInt64?
    let items: [AgentComposedChapterItem]
}

enum ChapterRawObservation: Equatable, Sendable {
    case publisher(PublisherChapterObservation)
    case model(ModelChapterObservation)
    case agent(AgentComposedChapterObservation)
}

enum ChapterCapabilityOutcome: Equatable, Sendable {
    case observed(
        observation: ChapterRawObservation,
        evidence: ChapterCapabilityEvidence,
        qualification: ChapterObservationProjection
    )
    case failed(ChapterCapabilityFailure)
}

enum ChapterCapabilityEvidence: Equatable, Sendable {
    case publisher(ChapterPublisherEvidence)
    case model(ChapterModelEvidence)
    case agent(ChapterAgentEvidence)
}

struct ChapterPublisherEvidence: Equatable, Sendable {
    let responseURL: String
    let contentType: String
    let entityTag: String?
    let lastModified: String?
    let httpStatus: UInt16
    let payloadDigest: ContentDigest
    let payloadByteCount: UInt64
}

struct ChapterModelEvidence: Equatable, Sendable {
    let provider: String
    let model: String
    let usage: ChapterModelUsage?
    let completionDigest: ContentDigest
    let completionByteCount: UInt64
}

struct ChapterAgentEvidence: Equatable, Sendable {
    let sourcePayloadDigest: ContentDigest
    let orderedItemCount: UInt32
}

struct ChapterCapabilityResponse: Equatable, Sendable {
    let requestID: HostRequestId
    let cancellationID: CancellationId
    let outcome: ChapterCapabilityOutcome
}

enum ChapterCapabilityFailureCode: Equatable, Sendable {
    case invalidRequest
    case transport
    case authentication
    case coreUnavailable
    case cancelled
    case responseTooLarge
    case invalidResponseMetadata
}

struct ChapterCapabilityFailure: Error, Equatable, Sendable {
    let code: ChapterCapabilityFailureCode
    let httpStatus: UInt16?
    let safeDetail: String?

    static let cancelled = Self(
        code: .cancelled,
        httpStatus: nil,
        safeDetail: nil
    )

    static let coreUnavailable = Self(
        code: .coreUnavailable,
        httpStatus: nil,
        safeDetail: "Chapter core is unavailable"
    )

    static func invalidRequest(_ detail: String) -> Self {
        Self(code: .invalidRequest, httpStatus: nil, safeDetail: detail)
    }

    static func responseTooLarge(_ detail: String) -> Self {
        Self(code: .responseTooLarge, httpStatus: nil, safeDetail: detail)
    }

    static func invalidMetadata(_ detail: String) -> Self {
        Self(code: .invalidResponseMetadata, httpStatus: nil, safeDetail: detail)
    }
}

protocol ChapterObservationQualifying: Sendable {
    func limits() -> ChapterObservationLimits?
    func qualify(_ observation: ChapterRawObservation) -> ChapterObservationProjection?
}

struct RustChapterObservationQualifier: ChapterObservationQualifying {
    func limits() -> ChapterObservationLimits? {
        chapterObservationLimits()
    }

    func qualify(_ observation: ChapterRawObservation) -> ChapterObservationProjection? {
        switch observation {
        case .publisher(let value):
            qualifyPublisherChapterObservation(observation: value)
        case .model(let value):
            qualifyModelChapterObservation(observation: value)
        case .agent(let value):
            qualifyAgentComposedChapterObservation(observation: value)
        }
    }
}

struct UnavailableChapterObservationQualifier: ChapterObservationQualifying {
    func limits() -> ChapterObservationLimits? { nil }
    func qualify(_: ChapterRawObservation) -> ChapterObservationProjection? { nil }
}
