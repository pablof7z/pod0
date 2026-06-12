import Foundation

// MARK: - Provider completion client

/// Stripped-down provider client tuned for JSON-shaped completion turns.
/// Live calls go through Rust provider transport; Swift only sends provider,
/// model, prompt, and response-format intent. Stub mode stays deterministic for
/// tests and previews.
struct ProviderCompletionClient: Sendable {

    private static let encoder = JSONEncoder()
    private static let decoder = JSONDecoder()


    // MARK: - Modes

    enum Mode: Sendable {
        /// Live mode — routes through Rust provider transport for `modelReference`.
        case live(modelReference: LLMModelReference)

        /// Stub mode — returns the supplied JSON string verbatim. Used
        /// by tests, previews, and the lane-7 stubbed pipeline path.
        case stubbed(json: String)
    }

    let mode: Mode

    // MARK: - Init

    init(
        mode: Mode,
        endpoint: URL? = nil,
        ollamaEndpoint: URL? = nil,
        urlSession: URLSession = .shared
    ) {
        _ = endpoint
        _ = ollamaEndpoint
        _ = urlSession
        self.mode = mode
    }

    // MARK: - Convenience constructors

    static func live(model: String = "openai/gpt-4o-mini") -> ProviderCompletionClient {
        ProviderCompletionClient(mode: .live(modelReference: LLMModelReference(storedID: model)))
    }

    static func stubbed(json: String) -> ProviderCompletionClient {
        ProviderCompletionClient(mode: .stubbed(json: json))
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
        case .live(let modelReference):
            return try await compileLive(
                systemPrompt: systemPrompt,
                userPrompt: userPrompt,
                feature: feature,
                modelReference: modelReference
            )
        }
    }

    // MARK: - Live request

    private func compileLive(
        systemPrompt: String,
        userPrompt: String,
        feature: String,
        modelReference: LLMModelReference
    ) async throws -> String {
        guard modelReference.provider != .local else {
            throw ProviderCompletionClientError.missingCredential(provider: modelReference.provider.displayName)
        }

        guard let handleBits = await MainActor.run(body: {
            KernelModel.shared?.podcastHandlePointer.map { Int(bitPattern: $0) }
        }) else {
            throw ProviderCompletionClientError.malformedResponse
        }

        let intent = ProviderCompletionIntent(
            provider: modelReference.provider.rawValue,
            model: modelReference.modelID,
            system: systemPrompt,
            user: userPrompt,
            responseFormat: "json_object"
        )
        let intentJSON = try Self.encoder.encode(intent)
        guard let intentString = String(data: intentJSON, encoding: .utf8) else {
            throw ProviderCompletionClientError.malformedResponse
        }
        let responseJSON = await Task.detached(priority: .userInitiated) {
            guard let handle = UnsafeMutableRawPointer(bitPattern: handleBits) else {
                return #"{"error":"Kernel handle unavailable"}"#
            }
            return intentString.withCString { intentPtr in
                guard let ptr = nmp_app_podcast_provider_complete(handle, intentPtr) else {
                    return #"{"error":"null response from Rust"}"#
                }
                defer { nmp_free_string(ptr) }
                return String(cString: ptr)
            }
        }.value

        guard let responseData = responseJSON.data(using: .utf8) else {
            throw ProviderCompletionClientError.malformedResponse
        }
        let envelope = try Self.decoder.decode(ProviderCompletionEnvelope.self, from: responseData)
        if let error = envelope.error {
            throw ProviderCompletionClientError.providerError(error)
        }
        guard let result = envelope.result else { throw ProviderCompletionClientError.malformedResponse }

        Task { @MainActor in
            let requestPreview = String(data: intentJSON, encoding: .utf8)
            switch modelReference.provider {
            case .openRouter:
                CostLedger.shared.log(
                    feature: feature,
                    model: result.model,
                    usage: result.usage,
                    latencyMs: result.latencyMs,
                    requestPayloadJSON: requestPreview,
                    responseContentPreview: result.text
                )
            case .ollama:
                CostLedger.shared.logOllama(
                    feature: feature,
                    model: result.model,
                    promptTokens: result.promptTokens,
                    completionTokens: result.completionTokens,
                    latencyMs: result.latencyMs,
                    requestPayloadJSON: requestPreview,
                    responseContentPreview: result.text
                )
            case .local:
                break
            }
        }

        return result.text
    }

    private struct ProviderCompletionIntent: Encodable {
        let provider: String
        let model: String
        let system: String
        let user: String
        let responseFormat: String

        private enum CodingKeys: String, CodingKey {
            case provider, model, system, user
            case responseFormat = "response_format"
        }
    }

    private struct ProviderCompletionEnvelope: Decodable {
        let result: ProviderCompletionResult?
        let error: String?
    }

    private struct ProviderCompletionResult: Decodable {
        let text: String
        let provider: String
        let model: String
        let latencyMs: Int
        let usage: OpenRouterUsagePayload?
        let promptTokens: Int
        let completionTokens: Int

        private enum CodingKeys: String, CodingKey {
            case text, provider, model, usage
            case latencyMs = "latency_ms"
            case promptTokens = "prompt_tokens"
            case completionTokens = "completion_tokens"
        }
    }
}

// MARK: - Errors

enum ProviderCompletionClientError: LocalizedError {
    case missingCredential(provider: String)
    case httpError(status: Int, body: String)
    case malformedResponse
    case providerError(String)

    var errorDescription: String? {
        switch self {
        case .missingCredential(let provider):
            "\(provider) is not connected. Add a key in Settings."
        case .httpError(let status, let body):
            "Provider completion error (\(status)): \(body.prefix(200))"
        case .malformedResponse:
            "Malformed response from provider completion"
        case .providerError(let message):
            "Provider completion error: \(message)"
        }
    }
}
