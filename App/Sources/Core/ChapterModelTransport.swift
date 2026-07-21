import Foundation
import Pod0Core

struct ChapterModelUsage: Equatable, Sendable {
    let promptTokens: Int
    let completionTokens: Int
    let cachedTokens: Int
    let reasoningTokens: Int
    let costUSD: Double?
}

struct ChapterModelTransportResponse: Equatable, Sendable {
    let completion: String
    let provider: String
    let model: String
    let usage: ChapterModelUsage?
}

protocol ChapterModelTransporting: Sendable {
    func execute(
        _ request: ModelChapterCapabilityRequest
    ) async -> Result<ChapterModelTransportResponse, ChapterCapabilityFailure>
}

protocol CoreChapterModelTransporting: Sendable {
    func execute(
        _ request: ChapterModelExecutionRequest
    ) async -> Result<ChapterModelTransportResponse, ChapterCapabilityFailure>
}

/// Credential-backed provider executor. It decodes only the provider envelope;
/// chapter JSON remains opaque until Rust qualification.
struct LiveChapterModelTransport: ChapterModelTransporting, CoreChapterModelTransporting, Sendable {
    typealias CredentialResolver = @Sendable (LLMProvider) throws -> String?

    private let session: URLSession
    private let openRouterEndpoint: URL
    private let ollamaEndpoint: URL
    private let credentialResolver: CredentialResolver
    private let now: @Sendable () -> Date

    init(
        session: URLSession = .shared,
        openRouterEndpoint: URL = UtilityLLMClient.defaultEndpoint,
        ollamaEndpoint: URL = UtilityLLMClient.defaultOllamaEndpoint,
        credentialResolver: @escaping CredentialResolver = {
            try LLMProviderCredentialResolver.apiKey(for: $0)
        },
        now: @escaping @Sendable () -> Date = Date.init
    ) {
        self.session = session
        self.openRouterEndpoint = openRouterEndpoint
        self.ollamaEndpoint = ollamaEndpoint
        self.credentialResolver = credentialResolver
        self.now = now
    }

    func execute(
        _ request: ModelChapterCapabilityRequest
    ) async -> Result<ChapterModelTransportResponse, ChapterCapabilityFailure> {
        await execute(ChapterModelExecutionRequest(
            provider: request.planned.provider,
            model: request.planned.model,
            systemPrompt: request.planned.systemPrompt,
            userPrompt: request.planned.userPrompt,
            responseFormat: request.planned.responseFormat,
            maximumCompletionBytes: request.planned.maximumCompletionBytes
        ))
    }

    func execute(
        _ request: ChapterModelExecutionRequest
    ) async -> Result<ChapterModelTransportResponse, ChapterCapabilityFailure> {
        guard request.maximumCompletionBytes > 0,
              request.responseFormat == .jsonObject,
              let provider = LLMProvider(rawValue: request.provider),
              !request.model.isEmpty,
              request.model.trimmed == request.model
        else {
            return .failure(.invalidRequest("Invalid chapter model identity"))
        }
        let apiKey: String
        do {
            guard let value = try credentialResolver(provider),
                  !value.isEmpty else {
                return .failure(ChapterCapabilityFailure(
                    code: .authentication,
                    httpStatus: nil,
                    safeDetail: "Chapter model credential unavailable"
                ))
            }
            apiKey = value
        } catch {
            return .failure(ChapterCapabilityFailure(
                code: .authentication,
                httpStatus: nil,
                safeDetail: "Chapter model credential unavailable"
            ))
        }

        let urlRequest: URLRequest
        do {
            urlRequest = try makeRequest(request, provider: provider, apiKey: apiKey)
        } catch {
            return .failure(.invalidRequest("Chapter model request encoding failed"))
        }
        return await send(
            urlRequest,
            provider: provider,
            maximumCompletionBytes: request.maximumCompletionBytes
        )
    }

    private func makeRequest(
        _ request: ChapterModelExecutionRequest,
        provider: LLMProvider,
        apiKey: String
    ) throws -> URLRequest {
        let endpoint = provider == .openRouter ? openRouterEndpoint : ollamaEndpoint
        var urlRequest = URLRequest(url: endpoint)
        urlRequest.httpMethod = "POST"
        urlRequest.timeoutInterval = 60
        urlRequest.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization")
        urlRequest.setValue("application/json", forHTTPHeaderField: "Content-Type")
        var body: [String: Any] = [
            "model": request.model,
            "messages": [
                ["role": "system", "content": request.systemPrompt],
                ["role": "user", "content": request.userPrompt],
            ],
            "stream": false,
        ]
        if provider == .openRouter {
            body["response_format"] = ["type": "json_object"]
        } else {
            body["format"] = "json"
        }
        urlRequest.httpBody = try JSONSerialization.data(withJSONObject: body)
        return urlRequest
    }

    private func send(
        _ request: URLRequest,
        provider: LLMProvider,
        maximumCompletionBytes: UInt64
    ) async -> Result<ChapterModelTransportResponse, ChapterCapabilityFailure> {
        let envelopeAllowance: UInt64 = 256 * 1_024
        let maximumEnvelopeBytes = maximumCompletionBytes.addingReportingOverflow(
            envelopeAllowance
        )
        guard !maximumEnvelopeBytes.overflow else {
            return .failure(.responseTooLarge("Invalid chapter model response limit"))
        }
        do {
            let (stream, response) = try await session.bytes(for: request)
            guard let http = response as? HTTPURLResponse,
                  let status = UInt16(exactly: http.statusCode)
            else {
                return .failure(.invalidMetadata("Non-HTTP chapter model response"))
            }
            guard (200...299).contains(http.statusCode) else {
                return .failure(Self.httpFailure(
                    status,
                    retryAfter: http.value(forHTTPHeaderField: "Retry-After"),
                    now: now()
                ))
            }
            if http.expectedContentLength > 0,
               UInt64(http.expectedContentLength) > maximumEnvelopeBytes.partialValue {
                return .failure(.responseTooLarge("Chapter model response exceeds limit"))
            }

            var data = Data()
            data.reserveCapacity(Int(min(maximumEnvelopeBytes.partialValue, 128 * 1_024)))
            for try await byte in stream {
                try Task.checkCancellation()
                guard UInt64(data.count) < maximumEnvelopeBytes.partialValue else {
                    return .failure(.responseTooLarge("Chapter model response exceeds limit"))
                }
                data.append(byte)
            }
            return decode(
                data,
                provider: provider,
                maximumCompletionBytes: maximumCompletionBytes
            )
        } catch is CancellationError {
            return .failure(.cancelled)
        } catch let error as URLError {
            if error.code == .cancelled { return .failure(.cancelled) }
            return .failure(ChapterCapabilityFailure(
                code: .transport,
                httpStatus: nil,
                safeDetail: "Chapter model transport failed"
            ))
        } catch {
            return .failure(ChapterCapabilityFailure(
                code: .transport,
                httpStatus: nil,
                safeDetail: "Chapter model transport failed"
            ))
        }
    }

    private func decode(
        _ data: Data,
        provider: LLMProvider,
        maximumCompletionBytes: UInt64
    ) -> Result<ChapterModelTransportResponse, ChapterCapabilityFailure> {
        do {
            let response: ChapterModelTransportResponse
            switch provider {
            case .openRouter:
                let envelope = try Self.decoder().decode(OpenRouterEnvelope.self, from: data)
                guard let content = envelope.choices.first?.message.content,
                      let model = envelope.model, !model.isEmpty else {
                    return .failure(.invalidMetadata("Malformed OpenRouter response"))
                }
                response = ChapterModelTransportResponse(
                    completion: content,
                    provider: provider.rawValue,
                    model: model,
                    usage: envelope.usage?.value
                )
            case .ollama:
                let envelope = try Self.decoder().decode(OllamaEnvelope.self, from: data)
                guard !envelope.model.isEmpty else {
                    return .failure(.invalidMetadata("Malformed Ollama response"))
                }
                response = ChapterModelTransportResponse(
                    completion: envelope.message.content,
                    provider: provider.rawValue,
                    model: envelope.model,
                    usage: ChapterModelUsage(
                        promptTokens: envelope.promptEvalCount ?? 0,
                        completionTokens: envelope.evalCount ?? 0,
                        cachedTokens: 0,
                        reasoningTokens: 0,
                        costUSD: nil
                    )
                )
            }
            guard UInt64(response.completion.utf8.count) <= maximumCompletionBytes else {
                return .failure(.responseTooLarge("Chapter model completion exceeds core limit"))
            }
            return .success(response)
        } catch {
            return .failure(.invalidMetadata("Malformed chapter model response"))
        }
    }

    private static func decoder() -> JSONDecoder {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return decoder
    }
}

private struct OpenRouterEnvelope: Decodable {
    struct Choice: Decodable {
        struct Message: Decodable { let content: String }
        let message: Message
    }
    struct Usage: Decodable {
        struct PromptDetails: Decodable { let cachedTokens: Int? }
        struct CompletionDetails: Decodable { let reasoningTokens: Int? }
        let promptTokens: Int?
        let completionTokens: Int?
        let cost: Double?
        let promptTokensDetails: PromptDetails?
        let completionTokensDetails: CompletionDetails?

        var value: ChapterModelUsage {
            ChapterModelUsage(
                promptTokens: promptTokens ?? 0,
                completionTokens: completionTokens ?? 0,
                cachedTokens: promptTokensDetails?.cachedTokens ?? 0,
                reasoningTokens: completionTokensDetails?.reasoningTokens ?? 0,
                costUSD: cost
            )
        }
    }
    let model: String?
    let choices: [Choice]
    let usage: Usage?
}

private struct OllamaEnvelope: Decodable {
    struct Message: Decodable { let content: String }
    let model: String
    let message: Message
    let promptEvalCount: Int?
    let evalCount: Int?
}
