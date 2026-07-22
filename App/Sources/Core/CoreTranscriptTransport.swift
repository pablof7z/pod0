import Foundation
import Pod0Core

enum CoreTranscriptTransportObservation: Sendable {
    case providerAccepted(externalOperationID: String, status: String?)
    case providerPending(status: String?, retryAfterMilliseconds: UInt64?)
    case completed(
        transcript: Transcript,
        externalOperationID: String?,
        status: String?
    )
}

enum CoreTranscriptTransportError: Error, Sendable, Equatable {
    case missingCredential
    case missingLocalAudio
    case invalidRequest
    case unsupportedProvider
    case publisherUnavailable
    case offline
    case rateLimited(retryAfterMilliseconds: UInt64?)
    case timedOut
    case transport
    case permissionDenied
    case providerRejected
    case providerUnavailable
    case responseTooLarge
    case invalidResponse
    case providerRecoveryUnavailable
}

protocol CoreTranscriptTransporting: Sendable {
    func execute(
        _ request: TranscriptCapabilityRequest
    ) async throws -> CoreTranscriptTransportObservation
}
