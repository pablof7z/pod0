import Foundation

extension OpenRouterWhisperClient.WhisperError: ProductFailureConvertible {
    var productFailure: ProductFailure {
        let code: ProductFailureCode
        switch self {
        case .missingAPIKey: code = .missingCredential
        case .invalidAudioURL: code = .invalidInput
        case .downloadFailed, .timedOut: code = .network
        case .http(let status, _) where status == 401 || status == 403:
            code = .missingCredential
        case .http(let status, _) where status == 415 || status == 422:
            code = .unsupportedFormat
        case .http(let status, _) where status == 429:
            code = .rateLimited
        case .http(let status, _) where status == 408 || status == 504 || status >= 500:
            code = .network
        case .cancelled: code = .cancelled
        case .invalidResponse, .http, .decoding: code = .unexpected
        }
        return ProductFailure(code: code)
    }
}

extension ElevenLabsScribeClient.ScribeError: ProductFailureConvertible {
    var productFailure: ProductFailure {
        let code: ProductFailureCode
        switch self {
        case .missingAPIKey: code = .missingCredential
        case .invalidAudioURL: code = .invalidInput
        case .http(let status, _) where status == 401 || status == 403:
            code = .missingCredential
        case .http(let status, _) where status == 415 || status == 422:
            code = .unsupportedFormat
        case .http(let status, _) where status == 429:
            code = .rateLimited
        case .http(let status, _) where status == 408 || status == 504 || status >= 500:
            code = .network
        case .cancelled: code = .cancelled
        case .timedOut: code = .network
        case .invalidResponse, .http, .decoding: code = .unexpected
        }
        return ProductFailure(code: code)
    }
}

extension AssemblyAITranscriptClient.TranscribeError: ProductFailureConvertible {
    var productFailure: ProductFailure {
        let code: ProductFailureCode
        switch self {
        case .missingAPIKey: code = .missingCredential
        case .invalidAudioURL: code = .invalidInput
        case .http(let status, _) where status == 401 || status == 403:
            code = .missingCredential
        case .http(let status, _) where status == 415 || status == 422:
            code = .unsupportedFormat
        case .http(let status, _) where status == 429:
            code = .rateLimited
        case .http(let status, _) where status == 408 || status == 504 || status >= 500:
            code = .network
        case .cancelled: code = .cancelled
        case .timedOut: code = .network
        case .invalidResponse, .http, .decoding, .remoteError: code = .unexpected
        }
        return ProductFailure(code: code)
    }
}
