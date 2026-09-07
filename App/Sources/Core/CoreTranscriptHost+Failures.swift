import Foundation
import Pod0Core

extension CoreTranscriptHost {
    func evidence(_ failure: CoreTranscriptTransportError) -> TranscriptFailureEvidence {
        switch failure {
        case .missingCredential: .missingCredential
        case .missingLocalAudio: .missingLocalAudio
        case .invalidRequest: .invalidRequest
        case .unsupportedProvider: .unsupportedProvider
        case .publisherUnavailable: .publisherUnavailable
        case .offline: .offline
        case .rateLimited: .rateLimited
        case .timedOut: .timedOut
        case .transport: .transport
        case .permissionDenied: .permissionDenied
        case .providerRejected: .providerRejected
        case .providerUnavailable: .providerUnavailable
        case .responseTooLarge: .responseTooLarge
        case .invalidResponse: .invalidResponse
        case .providerRecoveryUnavailable: .providerRecoveryUnavailable
        }
    }

    func evidence(_ failure: AssemblyAITranscriptClient.TranscribeError) -> TranscriptFailureEvidence {
        switch failure {
        case .missingAPIKey: .missingCredential
        case .invalidAudioURL: .invalidRequest
        case .http(let status): httpEvidence(status)
        case .timedOut: .timedOut
        case .cancelled: .cancelled
        case .remoteError: .providerRejected
        case .invalidResponse, .decoding: .invalidResponse
        }
    }

    func evidence(_ failure: ElevenLabsScribeClient.ScribeError) -> TranscriptFailureEvidence {
        switch failure {
        case .missingAPIKey: .missingCredential
        case .invalidAudioURL: .invalidRequest
        case .http(let status): httpEvidence(status)
        case .timedOut: .timedOut
        case .cancelled: .cancelled
        case .invalidResponse, .decoding: .invalidResponse
        }
    }

    func evidence(_ failure: OpenRouterWhisperClient.WhisperError) -> TranscriptFailureEvidence {
        switch failure {
        case .missingAPIKey: .missingCredential
        case .invalidAudioURL: .invalidRequest
        case .downloadFailed: .transport
        case .http(let status): httpEvidence(status)
        case .timedOut: .timedOut
        case .cancelled: .cancelled
        case .invalidResponse, .decoding: .invalidResponse
        }
    }

    func evidence(_ failure: AppleNativeSTTClient.STTError) -> TranscriptFailureEvidence {
        switch failure {
        case .notAuthorized: .permissionDenied
        case .requiresLocalFile, .audioFileUnreadable: .missingLocalAudio
        case .unavailable, .modelUnavailableForLocale: .providerUnavailable
        case .noResults: .invalidResponse
        }
    }

    func evidence(_ failure: URLError) -> TranscriptFailureEvidence {
        switch failure.code {
        case .notConnectedToInternet, .networkConnectionLost, .internationalRoamingOff:
            .offline
        case .timedOut: .timedOut
        case .userAuthenticationRequired, .userCancelledAuthentication: .permissionDenied
        case .dataLengthExceedsMaximum: .responseTooLarge
        case .cancelled: .cancelled
        default: .transport
        }
    }

    func httpEvidence(_ status: Int) -> TranscriptFailureEvidence {
        switch status {
        case 401, 403: .missingCredential
        case 408, 504: .timedOut
        case 429: .rateLimited
        case 500...599: .providerUnavailable
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

extension CoreTranscriptTransportError {
    var retryAfterMilliseconds: UInt64? {
        if case .rateLimited(let value) = self { value } else { nil }
    }
}
