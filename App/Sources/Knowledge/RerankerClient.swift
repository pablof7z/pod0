import Foundation
import os.log

// Lane 6 — RAG: Cohere `rerank-v3.5` passthrough via OpenRouter.
//
// OpenRouter exposes Cohere's rerank endpoint at `/api/v1/rerank` using the
// Cohere-compatible schema (model + query + documents). Returns relevance
// scores per document; we reorder the candidate indices client-side.
//
// Used as the optional final stage of `RAGSearch`: take the top-K hybrid
// results, rerank, take the top-N. Skipped under "rapid voice" latency
// budgets — per the spec, hybrid RRF alone is shippable quality.

/// Anything that can re-order candidate documents by relevance to a query.
protocol RerankerClient: Sendable {
    /// Given a `query` and a list of `documents`, return the indices of
    /// `documents` reordered from most-relevant to least-relevant. The
    /// returned array contains a permutation of `0..<documents.count`,
    /// possibly truncated to `topN` if specified.
    func rerank(query: String, documents: [String], topN: Int?) async throws -> [Int]
}

// MARK: - Settings-aware wrapper

struct SettingsAwareRerankerClient<Base: RerankerClient>: RerankerClient {
    private let base: Base
    private let isEnabled: @Sendable () async -> Bool

    init(
        base: Base,
        isEnabled: @Sendable @escaping () async -> Bool
    ) {
        self.base = base
        self.isEnabled = isEnabled
    }

    func rerank(
        query: String,
        documents: [String],
        topN: Int?
    ) async throws -> [Int] {
        guard await isEnabled() else {
            let limit = min(topN ?? documents.count, documents.count)
            return Array(0..<limit)
        }
        return try await base.rerank(query: query, documents: documents, topN: topN)
    }
}

// MARK: - OpenRouter implementation

struct OpenRouterRerankerClient: RerankerClient {
    static let defaultModel = "cohere/rerank-v3.5"

    private static let logger = Logger.app("OpenRouterRerankerClient")
    private static let endpoint = URL(string: "https://openrouter.ai/api/v1/rerank")!
    private static let xTitle = "Podcastr"

    private let apiKeyProvider: @Sendable () throws -> String?
    private let model: String
    private let session: URLSession
    private let timeout: TimeInterval

    init(
        apiKeyProvider: @Sendable @escaping () throws -> String? = { try OpenRouterCredentialStore.apiKey() },
        model: String = OpenRouterRerankerClient.defaultModel,
        session: URLSession = .shared,
        timeout: TimeInterval = 30
    ) {
        self.apiKeyProvider = apiKeyProvider
        self.model = model
        self.session = session
        self.timeout = timeout
    }

    func rerank(
        query: String,
        documents: [String],
        topN: Int? = nil
    ) async throws -> [Int] {
        guard !documents.isEmpty else { return [] }
        guard let apiKey = try apiKeyProvider(), !apiKey.isEmpty else {
            throw RerankerError.missingAPIKey
        }

        var req = URLRequest(url: Self.endpoint, timeoutInterval: timeout)
        req.httpMethod = "POST"
        req.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization")
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        req.setValue(Self.xTitle, forHTTPHeaderField: "X-Title")

        let payload = RequestPayload(
            model: model,
            query: query,
            documents: documents,
            top_n: topN ?? documents.count
        )
        req.httpBody = try JSONEncoder().encode(payload)

        let (data, response) = try await session.data(for: req)
        guard let http = response as? HTTPURLResponse else {
            throw RerankerError.transport(detail: "no HTTPURLResponse")
        }
        switch http.statusCode {
        case 200..<300:
            break
        case 401, 403:
            throw RerankerError.unauthorized
        case 429:
            throw RerankerError.rateLimited
        default:
            let body = String(data: data, encoding: .utf8) ?? "<binary>"
            Self.logger.warning("OpenRouter rerank HTTP \(http.statusCode, privacy: .public): \(body, privacy: .public)")
            throw RerankerError.serverError(statusCode: http.statusCode)
        }

        do {
            let decoded = try JSONDecoder().decode(ResponsePayload.self, from: data)
            // Cohere returns results sorted by relevance descending; we
            // just extract the original `index` field. Defensive sort by
            // score in case OpenRouter ever changes upstream behaviour.
            return decoded.results
                .sorted { $0.relevance_score > $1.relevance_score }
                .map(\.index)
        } catch {
            Self.logger.error("OpenRouter rerank decode failed: \(error, privacy: .public)")
            throw RerankerError.decoding
        }
    }

    // MARK: - DTOs

    private struct RequestPayload: Encodable {
        let model: String
        let query: String
        let documents: [String]
        let top_n: Int
    }

    private struct ResponsePayload: Decodable {
        let results: [ResultItem]
        struct ResultItem: Decodable {
            let index: Int
            let relevance_score: Double
        }
    }
}

// MARK: - Errors

enum RerankerError: LocalizedError {
    case missingAPIKey
    case unauthorized
    case rateLimited
    case serverError(statusCode: Int)
    case transport(detail: String)
    case decoding

    var errorDescription: String? {
        switch self {
        case .missingAPIKey:
            return "OpenRouter API key not configured. Add it in Settings → Intelligence → Providers."
        case .unauthorized:
            return "OpenRouter rejected the API key."
        case .rateLimited:
            return "OpenRouter is rate-limiting rerank requests. Try again shortly."
        case let .serverError(code):
            return "OpenRouter rerank returned HTTP \(code)."
        case let .transport(detail):
            return "Network error contacting OpenRouter: \(detail)."
        case .decoding:
            return "Could not decode the OpenRouter rerank response."
        }
    }
}
