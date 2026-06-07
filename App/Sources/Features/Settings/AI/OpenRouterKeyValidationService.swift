import Foundation
import os.log

// MARK: - Result type

struct OpenRouterKeyInfo: Sendable {
    var label: String?
    var usageDollars: Double?
    var limitDollars: Double?
    var isFreeTier: Bool
    var requestsPerInterval: Int?
    var rateInterval: String?

    /// Remaining credit as a fraction [0, 1], or nil if no limit is set.
    var remainingFraction: Double? {
        guard let limit = limitDollars, limit > 0, let usage = usageDollars else { return nil }
        return max(0, (limit - usage) / limit)
    }

    /// Human-readable remaining credit string.
    var remainingLabel: String? {
        guard let limit = limitDollars, let usage = usageDollars else { return nil }
        let remaining = limit - usage
        return String(format: "$%.4f remaining of $%.2f", max(0, remaining), limit)
    }
}

// MARK: - Service

struct OpenRouterKeyValidationService: Sendable {

    private static let logger = Logger.app("OpenRouterKeyValidationService")
    private static let decoder = JSONDecoder()

    func validateStoredKey() async throws -> OpenRouterKeyInfo {
        guard let handleBits = await MainActor.run(body: {
            KernelModel.shared?.podcastHandlePointer.map { Int(bitPattern: $0) }
        }) else {
            throw ValidationError.kernelUnavailable
        }

        let responseJSON = await Task.detached(priority: .userInitiated) {
            guard let handle = UnsafeMutableRawPointer(bitPattern: handleBits) else {
                return #"{"error":{"kind":"store_unavailable","message":"Kernel handle unavailable"}}"#
            }
            guard let ptr = nmp_app_podcast_validate_openrouter_key(handle) else {
                return #"{"error":{"kind":"store_unavailable","message":"null response from Rust"}}"#
            }
            defer { nmp_app_free_string(ptr) }
            return String(cString: ptr)
        }.value

        guard let responseData = responseJSON.data(using: .utf8) else {
            throw ValidationError.decodingError
        }

        do {
            let envelope = try Self.decoder.decode(OpenRouterValidationEnvelope.self, from: responseData)
            if let error = envelope.error {
                throw Self.validationError(from: error)
            }
            guard let result = envelope.result else {
                throw ValidationError.decodingError
            }
            return OpenRouterKeyInfo(
                label: result.label,
                usageDollars: result.usageDollars,
                limitDollars: result.limitDollars,
                isFreeTier: result.isFreeTier,
                requestsPerInterval: result.requestsPerInterval,
                rateInterval: result.rateInterval
            )
        } catch let error as ValidationError {
            throw error
        } catch {
            Self.logger.error("OpenRouter validation decode failed: \(error, privacy: .public)")
            throw ValidationError.decodingError
        }
    }

    // MARK: - DTOs

    private struct OpenRouterValidationEnvelope: Decodable {
        var result: OpenRouterValidationResult?
        var error: OpenRouterValidationBackendError?
    }

    private struct OpenRouterValidationResult: Decodable {
        var label: String?
        var usageDollars: Double?
        var limitDollars: Double?
        var isFreeTier: Bool
        var requestsPerInterval: Int?
        var rateInterval: String?

        enum CodingKeys: String, CodingKey {
            case label
            case usageDollars = "usage_dollars"
            case limitDollars = "limit_dollars"
            case isFreeTier = "is_free_tier"
            case requestsPerInterval = "requests_per_interval"
            case rateInterval = "rate_interval"
        }
    }

    private struct OpenRouterValidationBackendError: Decodable {
        var kind: String
        var message: String?
        var statusCode: Int?

        enum CodingKeys: String, CodingKey {
            case kind, message
            case statusCode = "status_code"
        }
    }

    private static func validationError(from error: OpenRouterValidationBackendError) -> ValidationError {
        switch error.kind {
        case "missing_api_key":
            return .missingKey
        case "invalid_key":
            return .invalidKey
        case "network_error":
            return .networkError
        case "server_error":
            return .serverError(statusCode: error.statusCode)
        case "decoding_error":
            return .decodingError
        case "store_unavailable":
            return .kernelUnavailable
        default:
            return .backend(error.message ?? "Unexpected OpenRouter validation error.")
        }
    }
}

// MARK: - Errors

enum ValidationError: LocalizedError {
    case missingKey
    case networkError
    case invalidKey
    case serverError(statusCode: Int?)
    case decodingError
    case kernelUnavailable
    case backend(String)

    var errorDescription: String? {
        switch self {
        case .missingKey:             return "No stored OpenRouter key found."
        case .networkError:           return "Could not reach OpenRouter. Check your connection."
        case .invalidKey:             return "Key rejected — check it is a valid OpenRouter API key."
        case .serverError(.some(let code)): return "OpenRouter returned an error (HTTP \(code))."
        case .serverError(.none):     return "OpenRouter returned an error."
        case .decodingError:          return "Unexpected response from OpenRouter."
        case .kernelUnavailable:      return "App backend is not ready yet. Try again in a moment."
        case .backend(let message):   return message
        }
    }
}
