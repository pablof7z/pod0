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

    private enum Constants {
        static let userURL = "https://api.elevenlabs.io/v1/user"
        static let timeout: TimeInterval = 15
        static let apiKeyHeader = "xi-api-key"
    }

    func validate(apiKey: String) async throws -> ElevenLabsKeyInfo {
        guard let url = URL(string: Constants.userURL) else {
            throw ElevenLabsValidationError.invalidURL
        }

        var request = URLRequest(url: url)
        request.setValue(apiKey, forHTTPHeaderField: Constants.apiKeyHeader)
        request.timeoutInterval = Constants.timeout

        let (data, response) = try await URLSession.shared.data(for: request)

        guard let http = response as? HTTPURLResponse else {
            throw ElevenLabsValidationError.networkError
        }

        switch http.statusCode {
        case 200..<300:
            break
        case 401, 403:
            throw ElevenLabsValidationError.invalidKey
        default:
            Self.logger.warning("ElevenLabs /v1/user returned \(http.statusCode, privacy: .public)")
            throw ElevenLabsValidationError.serverError(statusCode: http.statusCode)
        }

        do {
            let dto = try Self.decoder.decode(ELUserResponse.self, from: data)
            return ElevenLabsKeyInfo(
                tier: dto.subscription?.tier,
                characterCount: dto.subscription?.characterCount,
                characterLimit: dto.subscription?.characterLimit
            )
        } catch {
            Self.logger.error("ElevenLabs /v1/user decode failed: \(error, privacy: .public)")
            throw ElevenLabsValidationError.decodingError
        }
    }

    // MARK: - DTOs

    private struct ELUserResponse: Decodable {
        var subscription: ELSubscription?
    }

    private struct ELSubscription: Decodable {
        var tier: String?
        var characterCount: Int?
        var characterLimit: Int?

        enum CodingKeys: String, CodingKey {
            case tier
            case characterCount = "character_count"
            case characterLimit = "character_limit"
        }
    }
}

// MARK: - Errors

enum ElevenLabsValidationError: LocalizedError {
    case invalidURL
    case networkError
    case invalidKey
    case serverError(statusCode: Int)
    case decodingError

    var errorDescription: String? {
        switch self {
        case .invalidURL:             return "Invalid validation URL."
        case .networkError:           return "Could not reach ElevenLabs. Check your connection."
        case .invalidKey:             return "Key rejected — check it is a valid ElevenLabs API key."
        case .serverError(let code):  return "ElevenLabs returned an error (HTTP \(code))."
        case .decodingError:          return "Unexpected response from ElevenLabs."
        }
    }
}

