import Foundation
import Pod0Core

// Temporary agent-composition orchestration. Rust qualifies and persists the
// result; this shell owns only the in-flight native task until the agent
// workflow itself moves behind the shared host-request boundary.

struct ChapterCapabilityRequestEnvelope: Equatable, Sendable {
    let requestID: HostRequestId
    let cancellationID: CancellationId
    let request: ChapterCapabilityRequest
}

enum ChapterCapabilityRequest: Equatable, Sendable {
    case agent(AgentChapterCapabilityRequest)
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
    case agent(ChapterAgentEvidence)
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
    let retryAfterMilliseconds: UInt64?

    init(
        code: ChapterCapabilityFailureCode,
        httpStatus: UInt16?,
        safeDetail: String?,
        retryAfterMilliseconds: UInt64? = nil
    ) {
        self.code = code
        self.httpStatus = httpStatus
        self.safeDetail = safeDetail
        self.retryAfterMilliseconds = retryAfterMilliseconds
    }

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
        case .agent(let value):
            qualifyAgentComposedChapterObservation(observation: value)
        }
    }
}

struct UnavailableChapterObservationQualifier: ChapterObservationQualifying {
    func limits() -> ChapterObservationLimits? { nil }
    func qualify(_: ChapterRawObservation) -> ChapterObservationProjection? { nil }
}
