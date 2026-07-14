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

    private enum Constants {
        static let authKeyURL = "https://openrouter.ai/api/v1/auth/key"
        static let timeout: TimeInterval = 15
        static let xTitle = "Podcastr"
    }

    func validate(apiKey: String) async throws -> OpenRouterKeyInfo {
        guard let url = URL(string: Constants.authKeyURL) else {
            throw ValidationError.invalidURL
        }

        var request = URLRequest(url: url)
        request.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization")
        request.setValue(Constants.xTitle, forHTTPHeaderField: "X-Title")
        request.timeoutInterval = Constants.timeout

        let (data, response) = try await URLSession.shared.data(for: request)

        guard let http = response as? HTTPURLResponse else {
            throw ValidationError.networkError
        }

        switch http.statusCode {
        case 200..<300:
            break
        case 401, 403:
            throw ValidationError.invalidKey
        default:
            Self.logger.warning("OpenRouter auth/key returned \(http.statusCode, privacy: .public)")
            throw ValidationError.serverError(statusCode: http.statusCode)
        }

        do {
            let dto = try Self.decoder.decode(ORAuthKeyResponse.self, from: data)
            return OpenRouterKeyInfo(
                label: dto.data.label,
                usageDollars: dto.data.usage,
                limitDollars: dto.data.limit,
                isFreeTier: dto.data.isFreeTier,
                requestsPerInterval: dto.data.rateLimit?.requests,
                rateInterval: dto.data.rateLimit?.interval
            )
        } catch {
            Self.logger.error("OpenRouter auth/key decode failed: \(error, privacy: .public)")
            throw ValidationError.decodingError
        }
    }

    // MARK: - DTOs

    private struct ORAuthKeyResponse: Decodable {
        var data: ORAuthKeyData
    }

    private struct ORAuthKeyData: Decodable {
        var label: String?
        var usage: Double?
        var limit: Double?
        var isFreeTier: Bool
        var rateLimit: ORRateLimit?

        enum CodingKeys: String, CodingKey {
            case label, usage, limit
            case isFreeTier = "is_free_tier"
            case rateLimit = "rate_limit"
        }
    }

    private struct ORRateLimit: Decodable {
        var requests: Int?
        var interval: String?
    }
}

// MARK: - Errors

enum ValidationError: LocalizedError {
    case invalidURL
    case networkError
    case invalidKey
    case serverError(statusCode: Int)
    case decodingError

    var errorDescription: String? {
        switch self {
        case .invalidURL:             return "Invalid validation URL."
        case .networkError:           return "Could not reach OpenRouter. Check your connection."
        case .invalidKey:             return "Key rejected — check it is a valid OpenRouter API key."
        case .serverError(let code):  return "OpenRouter returned an error (HTTP \(code))."
        case .decodingError:          return "Unexpected response from OpenRouter."
        }
    }
}

