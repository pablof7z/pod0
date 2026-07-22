import Foundation
import Pod0Core

extension CoreTranscriptHost {
    func evidence(
        _ failure: CoreTranscriptTransportError,
        request: TranscriptCapabilityRequest
    ) -> TranscriptFailureEvidence {
        switch failure {
        case .missingCredential: .missingCredential
        case .missingLocalAudio: .missingLocalAudio
        case .invalidRequest: .invalidRequest
        case .unsupportedProvider: .unsupportedProvider
        case .publisherUnavailable: .publisherUnavailable
        case .offline: phaseEvidence(.offline, request: request)
        case .rateLimited: phaseEvidence(.rateLimited, request: request)
        case .timedOut: phaseEvidence(.timedOut, request: request)
        case .transport: phaseEvidence(.transport, request: request)
        case .permissionDenied: .permissionDenied
        case .providerRejected: .providerRejected
        case .providerUnavailable: phaseEvidence(.providerUnavailable, request: request)
        case .responseTooLarge: .responseTooLarge
        case .invalidResponse: .invalidResponse
        case .providerRecoveryUnavailable: .providerRecoveryUnavailable
        }
    }

    func evidence(
        _ failure: AssemblyAITranscriptClient.TranscribeError,
        request: TranscriptCapabilityRequest
    ) -> TranscriptFailureEvidence {
        switch failure {
        case .missingAPIKey: .missingCredential
        case .invalidAudioURL: .invalidRequest
        case .http(let status): httpEvidence(status, request: request)
        case .timedOut: phaseEvidence(.timedOut, request: request)
        case .cancelled: phaseEvidence(.cancelled, request: request)
        case .remoteError: .providerRejected
        case .invalidResponse, .decoding: .invalidResponse
        }
    }

    func evidence(
        _ failure: ElevenLabsScribeClient.ScribeError,
        request: TranscriptCapabilityRequest
    ) -> TranscriptFailureEvidence {
        switch failure {
        case .missingAPIKey: .missingCredential
        case .invalidAudioURL: .invalidRequest
        case .http(let status): httpEvidence(status, request: request)
        case .timedOut: phaseEvidence(.timedOut, request: request)
        case .cancelled: phaseEvidence(.cancelled, request: request)
        case .invalidResponse, .decoding: .invalidResponse
        }
    }

    func evidence(
        _ failure: OpenRouterWhisperClient.WhisperError,
        request: TranscriptCapabilityRequest
    ) -> TranscriptFailureEvidence {
        switch failure {
        case .missingAPIKey: .missingCredential
        case .invalidAudioURL: .invalidRequest
        case .downloadFailed: phaseEvidence(.transport, request: request)
        case .http(let status): httpEvidence(status, request: request)
        case .timedOut: phaseEvidence(.timedOut, request: request)
        case .cancelled: phaseEvidence(.cancelled, request: request)
        case .invalidResponse, .decoding: .invalidResponse
        }
    }

    func evidence(_ failure: AppleNativeSTTClient.STTError) -> TranscriptFailureEvidence {
        switch failure {
        case .notAuthorized: .permissionDenied
        case .requiresLocalFile, .audioFileUnreadable: .missingLocalAudio
        case .unavailable, .modelUnavailableForLocale: .providerUnavailable(
            submissionAuthorized: false,
            providerAccepted: false
        )
        case .noResults: .invalidResponse
        }
    }

    func evidence(
        _ failure: URLError,
        request: TranscriptCapabilityRequest
    ) -> TranscriptFailureEvidence {
        switch failure.code {
        case .notConnectedToInternet, .networkConnectionLost, .internationalRoamingOff:
            phaseEvidence(.offline, request: request)
        case .timedOut: phaseEvidence(.timedOut, request: request)
        case .userAuthenticationRequired, .userCancelledAuthentication: .permissionDenied
        case .dataLengthExceedsMaximum: .responseTooLarge
        case .cancelled: phaseEvidence(.cancelled, request: request)
        default: phaseEvidence(.transport, request: request)
        }
    }

    func phaseEvidence(
        _ kind: CoreTranscriptPhaseFailure,
        request: TranscriptCapabilityRequest
    ) -> TranscriptFailureEvidence {
        let phase = request.providerPhase
        return switch kind {
        case .offline:
            .offline(submissionAuthorized: phase.authorized, providerAccepted: phase.accepted)
        case .rateLimited:
            .rateLimited(submissionAuthorized: phase.authorized, providerAccepted: phase.accepted)
        case .timedOut:
            .timedOut(submissionAuthorized: phase.authorized, providerAccepted: phase.accepted)
        case .transport:
            .transport(submissionAuthorized: phase.authorized, providerAccepted: phase.accepted)
        case .providerUnavailable:
            .providerUnavailable(
                submissionAuthorized: phase.authorized,
                providerAccepted: phase.accepted
            )
        case .cancelled:
            .cancelled(submissionAuthorized: phase.authorized, providerAccepted: phase.accepted)
        }
    }

    func httpEvidence(
        _ status: Int,
        request: TranscriptCapabilityRequest
    ) -> TranscriptFailureEvidence {
        switch status {
        case 401, 403: .missingCredential
        case 408, 504: phaseEvidence(.timedOut, request: request)
        case 429: phaseEvidence(.rateLimited, request: request)
        case 500...599: phaseEvidence(.providerUnavailable, request: request)
        case 413: .responseTooLarge
        case 400...499: .providerRejected
        default: .invalidResponse
        }
    }

    func safeDetail(_ failure: CoreTranscriptTransportError) -> String {
        switch failure {
        case .missingCredential: "Transcript credential is unavailable"
        case .missingLocalAudio: "Local audio is unavailable"
        case .invalidRequest: "Transcript request is invalid"
        case .unsupportedProvider: "Transcript provider is unsupported"
        case .publisherUnavailable: "Publisher transcript is unavailable"
        case .offline: "Network is offline"
        case .rateLimited: "Transcript provider rate limited the request"
        case .timedOut: "Transcript provider request timed out"
        case .transport: "Transcript transport failed"
        case .permissionDenied: "Transcript permission was denied"
        case .providerRejected: "Transcript provider rejected the request"
        case .providerUnavailable: "Transcript provider is unavailable"
        case .responseTooLarge: "Transcript response exceeds the core limit"
        case .invalidResponse: "Transcript provider returned an invalid response"
        case .providerRecoveryUnavailable: "Transcript provider recovery is unavailable"
        }
    }
}

enum CoreTranscriptPhaseFailure {
    case offline, rateLimited, timedOut, transport, providerUnavailable, cancelled
}

private extension TranscriptCapabilityRequest {
    var providerPhase: (authorized: Bool, accepted: Bool) {
        switch self {
        case .submitProvider: (true, false)
        case .recoverProvider: (true, true)
        case .fetchPublisher, .transcribeLocal: (false, false)
        }
    }
}

extension CoreTranscriptTransportError {
    var retryAfterMilliseconds: UInt64? {
        if case .rateLimited(let value) = self { value } else { nil }
    }
}
