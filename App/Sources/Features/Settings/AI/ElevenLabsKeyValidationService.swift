import Foundation
import os.log

// MARK: - Result type

struct ElevenLabsKeyInfo: Sendable {
    /// Human-readable subscription tier (e.g. "free", "starter", "creator").
    var tier: String?
    /// Characters consumed in the current billing period.
    var characterCount: Int?
    /// Total character allowance for the current billing period.
    var characterLimit: Int?

    /// Remaining characters as a fraction [0, 1], or nil when limit is unknown.
    var remainingFraction: Double? {
        guard let limit = characterLimit, limit > 0, let count = characterCount else { return nil }
        return max(0, Double(limit - count) / Double(limit))
    }

    /// Human-readable remaining character label.
    var remainingLabel: String? {
        guard let limit = characterLimit, let count = characterCount else { return nil }
        let remaining = max(0, limit - count)
        let remainStr = Self.decimalFormatter.string(from: NSNumber(value: remaining)) ?? "\(remaining)"
        let limitStr = Self.decimalFormatter.string(from: NSNumber(value: limit)) ?? "\(limit)"
        return "\(remainStr) of \(limitStr) chars remaining"
    }

    /// Cached decimal formatter — `NumberFormatter` is expensive to allocate and thread-safe for reads after setup.
    private static let decimalFormatter: NumberFormatter = {
        let f = NumberFormatter()
        f.numberStyle = .decimal
        return f
    }()
}

// MARK: - Service

struct ElevenLabsKeyValidationService: Sendable {

    private static let logger = Logger.app("ElevenLabsKeyValidationService")
    private static let decoder = JSONDecoder()

    func validateStoredKey() async throws -> ElevenLabsKeyInfo {
        guard let handleBits = await MainActor.run(body: {
            KernelModel.shared?.podcastHandlePointer.map { Int(bitPattern: $0) }
        }) else {
            throw ElevenLabsValidationError.kernelUnavailable
        }

        let responseJSON = await Task.detached(priority: .userInitiated) {
            guard let handle = UnsafeMutableRawPointer(bitPattern: handleBits) else {
                return #"{"error":{"kind":"store_unavailable","message":"Kernel handle unavailable"}}"#
            }
            guard let ptr = nmp_app_podcast_validate_elevenlabs_key(handle) else {
                return #"{"error":{"kind":"store_unavailable","message":"null response from Rust"}}"#
            }
            defer { nmp_app_free_string(ptr) }
            return String(cString: ptr)
        }.value

        guard let responseData = responseJSON.data(using: .utf8) else {
            throw ElevenLabsValidationError.decodingError
        }

        do {
            let envelope = try Self.decoder.decode(ElevenLabsValidationEnvelope.self, from: responseData)
            if let error = envelope.error {
                throw Self.validationError(from: error)
            }
            guard let result = envelope.result else {
                throw ElevenLabsValidationError.decodingError
            }
            return ElevenLabsKeyInfo(
                tier: result.tier,
                characterCount: result.characterCount,
                characterLimit: result.characterLimit
            )
        } catch let error as ElevenLabsValidationError {
            throw error
        } catch {
            Self.logger.error("ElevenLabs validation decode failed: \(error, privacy: .public)")
            throw ElevenLabsValidationError.decodingError
        }
    }

    // MARK: - DTOs

    private struct ElevenLabsValidationEnvelope: Decodable {
        var result: ElevenLabsValidationResult?
        var error: ElevenLabsValidationBackendError?
    }

    private struct ElevenLabsValidationResult: Decodable {
        var tier: String?
        var characterCount: Int?
        var characterLimit: Int?

        enum CodingKeys: String, CodingKey {
            case tier
            case characterCount = "character_count"
            case characterLimit = "character_limit"
        }
    }

    private struct ElevenLabsValidationBackendError: Decodable {
        var kind: String
        var message: String?
        var statusCode: Int?

        enum CodingKeys: String, CodingKey {
            case kind, message
            case statusCode = "status_code"
        }
    }

    private static func validationError(from error: ElevenLabsValidationBackendError) -> ElevenLabsValidationError {
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
            return .backend(error.message ?? "Unexpected ElevenLabs validation error.")
        }
    }
}

// MARK: - Errors

enum ElevenLabsValidationError: LocalizedError {
    case missingKey
    case networkError
    case invalidKey
    case serverError(statusCode: Int?)
    case decodingError
    case kernelUnavailable
    case backend(String)

    var errorDescription: String? {
        switch self {
        case .missingKey:             return "No stored ElevenLabs key found."
        case .networkError:           return "Could not reach ElevenLabs. Check your connection."
        case .invalidKey:             return "Key rejected — check it is a valid ElevenLabs API key."
        case .serverError(.some(let code)): return "ElevenLabs returned an error (HTTP \(code))."
        case .serverError(.none):     return "ElevenLabs returned an error."
        case .decodingError:          return "Unexpected response from ElevenLabs."
        case .kernelUnavailable:      return "App backend is not ready yet. Try again in a moment."
        case .backend(let message):   return message
        }
    }
}
