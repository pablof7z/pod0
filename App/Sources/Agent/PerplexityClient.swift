import Foundation
import os.log

/// Concrete `PerplexityClientProtocol` — wraps the Perplexity online-search API.
///
/// Reads its bearer token from `PerplexityCredentialStore`, the typed
/// counterpart of `OpenRouterCredentialStore` / `ElevenLabsCredentialStore`.
/// If no key is stored the client throws `PerplexityClientError.missingAPIKey`
/// rather than calling out — callers are expected to surface a "needs setup"
/// affordance to the user.
public actor PerplexityClient: PerplexityClientProtocol {

    // MARK: - Keychain contract (legacy aliases)

    /// Legacy alias; new code should go through `PerplexityCredentialStore`.
    public static let keychainService: String = PerplexityCredentialStore.service
    /// Legacy alias; new code should go through `PerplexityCredentialStore`.
    public static let keychainAccount: String = PerplexityCredentialStore.account

    // MARK: - Endpoint

    /// Default Perplexity chat-completions endpoint. Exposed for tests.
    public static let defaultEndpoint = URL(string: "https://api.perplexity.ai/chat/completions")!
    /// Default model the client requests. Conservative — small + online.
    public static let defaultModel = "sonar-small-online"

    /// OpenRouter endpoint used when routing Perplexity searches via OpenRouter.
    public static let openRouterEndpoint = URL(string: "https://openrouter.ai/api/v1/chat/completions")!
    /// Perplexity model identifier as used on OpenRouter.
    public static let openRouterModel = "perplexity/sonar"

    private let endpoint: URL
    private let model: String
    private let session: URLSession
    private let logger = Logger.app("PerplexityClient")

    public init(
        endpoint: URL = PerplexityClient.defaultEndpoint,
        model: String = PerplexityClient.defaultModel,
        session: URLSession = .shared
    ) {
        self.endpoint = endpoint
        self.model = model
        self.session = session
    }

    // MARK: - PerplexityClientProtocol

    public func search(query: String) async throws -> PerplexityResult {
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            throw PerplexityClientError.invalidQuery
        }

        let (apiKey, resolvedEndpoint, resolvedModel) = try Self.resolveRequest(
            defaultEndpoint: endpoint, defaultModel: model
        )

        var request = URLRequest(url: resolvedEndpoint)
        request.httpMethod = "POST"
        request.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization")
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        // Perplexity searches do a web crawl + LLM synthesis pass and can
        // legitimately take 30–60 seconds on busy queries. The default
        // `URLRequest.timeoutInterval` is 60s, which leaves no headroom —
        // long-tail queries time out before the response lands. Bumping
        // to 90s gives the slowest legitimate responses room to return.
        request.timeoutInterval = 90

        let body: [String: Any] = [
            "model": resolvedModel,
            "messages": [
                ["role": "user", "content": trimmed],
            ],
            "return_citations": true,
        ]
        request.httpBody = try JSONSerialization.data(withJSONObject: body)

        let (data, response) = try await session.data(for: request)
        guard let http = response as? HTTPURLResponse else {
            throw PerplexityClientError.transport("non-HTTP response")
        }
        guard (200..<300).contains(http.statusCode) else {
            let bodyText = String(data: data, encoding: .utf8) ?? ""
            logger.error("Perplexity HTTP \(http.statusCode, privacy: .public): \(bodyText, privacy: .public)")
            throw PerplexityClientError.httpStatus(http.statusCode)
        }

        return try Self.parseResponse(data)
    }

    // MARK: - Helpers

    /// Resolves which endpoint, model, and API key to use for a search request.
    ///
    /// Priority order:
    /// 1. If a custom (test) endpoint was provided, use the direct Perplexity key with it.
    /// 2. If an OpenRouter key is stored, route through OpenRouter using `perplexity/sonar` —
    ///    no dedicated Perplexity key required.
    /// 3. Fall back to a direct Perplexity API key.
    static func resolveRequest(
        defaultEndpoint: URL,
        defaultModel: String
    ) throws -> (apiKey: String, endpoint: URL, model: String) {
        // Custom endpoint means a test override — skip the resolver entirely.
        if defaultEndpoint != Self.defaultEndpoint {
            return (try readAPIKey(), defaultEndpoint, defaultModel)
        }
        // Prefer OpenRouter if a key is available.
        if let orKey = try? OpenRouterCredentialStore.apiKey(), !orKey.isEmpty {
            return (orKey, openRouterEndpoint, openRouterModel)
        }
        // Fall back to a dedicated Perplexity key.
        return (try readAPIKey(), defaultEndpoint, defaultModel)
    }

    /// Reads the API key via `PerplexityCredentialStore`. Throws if no key
    /// is present so the agent's `perplexity_search` tool can surface a
    /// clean error to the model.
    static func readAPIKey() throws -> String {
        let stored: String?
        do {
            stored = try PerplexityCredentialStore.apiKey()
        } catch {
            throw PerplexityClientError.keychain(error.localizedDescription)
        }
        guard let key = stored, !key.isEmpty else {
            throw PerplexityClientError.missingAPIKey
        }
        return key
    }

    /// Parses Perplexity's chat-completions JSON into a `PerplexityResult`.
    /// Tolerant: missing citations array becomes `[]`, missing answer becomes
    /// the empty string. Exposed `internal` for tests.
    static func parseResponse(_ data: Data) throws -> PerplexityResult {
        let raw = try JSONSerialization.jsonObject(with: data)
        guard let root = raw as? [String: Any] else {
            throw PerplexityClientError.malformedResponse("root not an object")
        }

        // Answer text — choices[0].message.content
        var answer = ""
        if let choices = root["choices"] as? [[String: Any]],
           let first = choices.first,
           let message = first["message"] as? [String: Any],
           let content = message["content"] as? String {
            answer = content
        }

        // Citations — Perplexity returns them either as a top-level "citations"
        // array of URL strings, or inside an "search_results" array of objects.
        var sources: [PerplexityResult.Source] = []
        if let urls = root["citations"] as? [String] {
            sources = urls.map { PerplexityResult.Source(title: $0, url: $0) }
        } else if let results = root["search_results"] as? [[String: Any]] {
            sources = results.compactMap { obj in
                guard let url = obj["url"] as? String else { return nil }
                let title = (obj["title"] as? String) ?? url
                return PerplexityResult.Source(title: title, url: url)
            }
        }

        return PerplexityResult(answer: answer, sources: sources)
    }
}

// MARK: - Errors

public enum PerplexityClientError: LocalizedError {
    case invalidQuery
    case missingAPIKey
    case keychain(String)
    case transport(String)
    case httpStatus(Int)
    case malformedResponse(String)

    public var errorDescription: String? {
        switch self {
        case .invalidQuery:
            return "Empty Perplexity query."
        case .missingAPIKey:
            return "No Perplexity or OpenRouter key configured. Add one in Settings → Providers."
        case .keychain(let detail):
            return "Keychain error reading Perplexity key: \(detail)"
        case .transport(let detail):
            return "Network error talking to Perplexity: \(detail)"
        case .httpStatus(let code):
            return "Perplexity returned HTTP \(code)."
        case .malformedResponse(let detail):
            return "Couldn't parse Perplexity response: \(detail)"
        }
    }
}
