import Foundation

// MARK: - Wiki LLM client

/// Stripped-down provider client tuned for the wiki
/// compile pipeline.
///
/// Differs from `AgentOpenRouterClient` (which streams SSE deltas and
/// supports tool calls) on purpose:
///   • Compile turns are non-interactive — no streaming UI required.
///   • The response is forced to a single JSON object via
///     `response_format: { "type": "json_object" }`.
///   • There are no tool calls — the only side effect is the synthesis.
///
/// `Agent/` is intentionally untouched; we duplicate the small request
/// scaffolding rather than couple the wiki pipeline to the agent loop.
///
/// All real network calls are gated behind the `live` initialiser.
/// The `stubbed` initialiser returns deterministic fixture JSON so the
/// generator pipeline is exercisable in tests and previews without an
/// API key (lane 7's "real LLM calls are stubbed" constraint).
struct WikiOpenRouterClient: Sendable {

    /// Shared decoder for the per-call `OpenRouterUsagePayload` parse —
    /// the previous shape allocated one per wiki compile, on top of the
    /// already-expensive LLM round-trip. Tiny win individually; matches
    /// the pattern other clients in the codebase already follow.
    nonisolated(unsafe) private static let usageDecoder = JSONDecoder()


    // MARK: - Modes

    enum Mode: Sendable {
        /// Live mode — POSTs to the provider encoded in `modelReference`.
        case live(apiKey: String?, modelReference: LLMModelReference)

        /// Stub mode — returns the supplied JSON string verbatim. Used
        /// by tests, previews, and the lane-7 stubbed pipeline path.
        case stubbed(json: String)
    }

    let mode: Mode
    let urlSession: URLSession
    let endpoint: URL
    let ollamaEndpoint: URL

    static let defaultEndpoint = URL(string: "https://openrouter.ai/api/v1/chat/completions")!
    static let defaultOllamaEndpoint = URL(string: "https://ollama.com/api/chat")!

    // MARK: - Init

    init(
        mode: Mode,
        endpoint: URL = WikiOpenRouterClient.defaultEndpoint,
        ollamaEndpoint: URL = WikiOpenRouterClient.defaultOllamaEndpoint,
        urlSession: URLSession = .shared
    ) {
        self.mode = mode
        self.endpoint = endpoint
        self.ollamaEndpoint = ollamaEndpoint
        self.urlSession = urlSession
    }

    // MARK: - Convenience constructors

    static func live(apiKey: String, model: String = "openai/gpt-4o-mini") -> WikiOpenRouterClient {
        WikiOpenRouterClient(mode: .live(apiKey: apiKey, modelReference: LLMModelReference(storedID: model)))
    }

    static func live(model: String = "openai/gpt-4o-mini") -> WikiOpenRouterClient {
        WikiOpenRouterClient(mode: .live(apiKey: nil, modelReference: LLMModelReference(storedID: model)))
    }

    static func stubbed(json: String) -> WikiOpenRouterClient {
        WikiOpenRouterClient(mode: .stubbed(json: json))
    }

    // MARK: - Public API

    /// Sends a system + user prompt to the selected provider and returns the raw
    /// JSON content of the assistant message. Caller is responsible for
    /// decoding (see `WikiResponseParser`).
    ///
    /// In stubbed mode returns the stored fixture JSON unchanged.
    func compile(
        systemPrompt: String,
        userPrompt: String,
        feature: String = CostFeature.wikiCompile
    ) async throws -> String {
        switch mode {
        case .stubbed(let json):
            return json
        case .live(let apiKey, let modelReference):
            return try await compileLive(
                systemPrompt: systemPrompt,
                userPrompt: userPrompt,
                feature: feature,
                apiKey: apiKey,
                modelReference: modelReference
            )
        }
    }

    // MARK: - Live request

    private func compileLive(
        systemPrompt: String,
        userPrompt: String,
        feature: String,
        apiKey: String?,
        modelReference: LLMModelReference
    ) async throws -> String {
        let resolvedKey: String
        if let apiKey, !apiKey.isEmpty {
            resolvedKey = apiKey
        } else if let key = try LLMProviderCredentialResolver.apiKey(for: modelReference.provider), !key.isEmpty {
            resolvedKey = key
        } else {
            throw WikiClientError.missingCredential(provider: modelReference.provider.displayName)
        }

        switch modelReference.provider {
        case .openRouter:
            return try await compileOpenRouter(
                systemPrompt: systemPrompt,
                userPrompt: userPrompt,
                feature: feature,
                apiKey: resolvedKey,
                model: modelReference.modelID
            )
        case .ollama:
            return try await compileOllama(
                systemPrompt: systemPrompt,
                userPrompt: userPrompt,
                feature: feature,
                apiKey: resolvedKey,
                model: modelReference.modelID
            )
        }
    }

    private func compileOpenRouter(
        systemPrompt: String,
        userPrompt: String,
        feature: String,
        apiKey: String,
        model: String
    ) async throws -> String {
        var request = URLRequest(url: endpoint)
        request.httpMethod = "POST"
        request.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization")
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.timeoutInterval = 60

        let body: [String: Any] = [
            "model": model,
            "messages": [
                ["role": "system", "content": systemPrompt],
                ["role": "user", "content": userPrompt],
            ],
            "response_format": ["type": "json_object"],
            "stream": false,
        ]
        let bodyData = try JSONSerialization.data(withJSONObject: body)
        request.httpBody = bodyData
        let requestPayloadJSON = String(data: bodyData, encoding: .utf8)

        let start = Date()
        let (data, response) = try await urlSession.data(for: request)
        let latencyMs = Int(Date().timeIntervalSince(start) * 1000)

        guard let http = response as? HTTPURLResponse else {
            throw WikiClientError.malformedResponse
        }
        guard (200..<300).contains(http.statusCode) else {
            let bodyString = String(data: data, encoding: .utf8) ?? ""
            throw WikiClientError.httpError(status: http.statusCode, body: bodyString)
        }

        guard
            let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
            let choices = json["choices"] as? [[String: Any]],
            let message = choices.first?["message"] as? [String: Any],
            let content = message["content"] as? String
        else {
            throw WikiClientError.malformedResponse
        }

        if let usageRaw = json["usage"] {
            let usageData = try? JSONSerialization.data(withJSONObject: usageRaw)
            let usage = usageData.flatMap { try? Self.usageDecoder.decode(OpenRouterUsagePayload.self, from: $0) }
            let modelUsed = (json["model"] as? String) ?? model
            Task { @MainActor in
                CostLedger.shared.log(
                    feature: feature,
                    model: modelUsed,
                    usage: usage,
                    latencyMs: latencyMs,
                    requestPayloadJSON: requestPayloadJSON,
                    responseContentPreview: content
                )
            }
        }

        return content
    }

    private func compileOllama(
        systemPrompt: String,
        userPrompt: String,
        feature: String,
        apiKey: String,
        model: String
    ) async throws -> String {
        var request = URLRequest(url: ollamaEndpoint)
        request.httpMethod = "POST"
        request.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization")
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.timeoutInterval = 60

        let body: [String: Any] = [
            "model": model,
            "messages": [
                ["role": "system", "content": systemPrompt],
                ["role": "user", "content": userPrompt],
            ],
            "format": "json",
            "stream": false,
        ]
        let bodyData = try JSONSerialization.data(withJSONObject: body)
        request.httpBody = bodyData
        let requestPayloadJSON = String(data: bodyData, encoding: .utf8)

        let start = Date()
        let (data, response) = try await urlSession.data(for: request)
        let latencyMs = Int(Date().timeIntervalSince(start) * 1000)

        guard let http = response as? HTTPURLResponse else {
            throw WikiClientError.malformedResponse
        }
        guard (200..<300).contains(http.statusCode) else {
            let bodyString = String(data: data, encoding: .utf8) ?? ""
            throw WikiClientError.httpError(status: http.statusCode, body: bodyString)
        }

        guard
            let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
            let message = json["message"] as? [String: Any],
            let content = message["content"] as? String
        else {
            throw WikiClientError.malformedResponse
        }

        let promptTokens = (json["prompt_eval_count"] as? Int) ?? 0
        let completionTokens = (json["eval_count"] as? Int) ?? 0
        let modelUsed = (json["model"] as? String) ?? model
        Task { @MainActor in
            CostLedger.shared.logOllama(
                feature: feature,
                model: modelUsed,
                promptTokens: promptTokens,
                completionTokens: completionTokens,
                latencyMs: latencyMs,
                requestPayloadJSON: requestPayloadJSON,
                responseContentPreview: content
            )
        }

        return content
    }
}

// MARK: - Errors

enum WikiClientError: LocalizedError {
    case missingCredential(provider: String)
    case httpError(status: Int, body: String)
    case malformedResponse

    var errorDescription: String? {
        switch self {
        case .missingCredential(let provider):
            "\(provider) is not connected. Add a key in Settings."
        case .httpError(let status, let body):
            "Wiki API error (\(status)): \(body.prefix(200))"
        case .malformedResponse:
            "Malformed response from wiki API"
        }
    }
}
