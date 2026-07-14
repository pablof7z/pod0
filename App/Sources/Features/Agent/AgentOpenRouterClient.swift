import Foundation

// MARK: - Stream accumulator

/// Accumulates SSE delta chunks from a streaming OpenRouter response into a
/// complete assistant message and zero or more tool calls.
struct StreamAccumulator {
    var content: String = ""
    var toolCalls: [Int: PartialCall] = [:]

    struct PartialCall {
        var id: String = ""
        var name: String = ""
        var arguments: String = ""
    }

    mutating func applyDelta(_ delta: [String: Any]) {
        if let text = delta["content"] as? String {
            content += text
        }
        if let chunks = delta["tool_calls"] as? [[String: Any]] {
            for chunk in chunks {
                let idx = chunk["index"] as? Int ?? 0
                if toolCalls[idx] == nil { toolCalls[idx] = PartialCall() }
                if let id = chunk["id"] as? String, !id.isEmpty { toolCalls[idx]?.id = id }
                if let fn = chunk["function"] as? [String: Any] {
                    if let name = fn["name"] as? String { toolCalls[idx]?.name += name }
                    if let args = fn["arguments"] as? String { toolCalls[idx]?.arguments += args }
                }
            }
        }
    }

    func toResult() -> AgentResult {
        let agentCalls = toolCalls.sorted { $0.key < $1.key }.compactMap { (_, call) -> AgentToolCall? in
            guard !call.id.isEmpty, !call.name.isEmpty else { return nil }
            return AgentToolCall(id: call.id, name: call.name, arguments: call.arguments)
        }
        var assistantMsg: [String: Any] = ["role": "assistant"]
        if !content.isEmpty { assistantMsg["content"] = content }
        if !agentCalls.isEmpty {
            assistantMsg["tool_calls"] = toolCalls.sorted { $0.key < $1.key }.compactMap { (_, call) -> [String: Any]? in
                guard !call.id.isEmpty else { return nil }
                return [
                    "id": call.id,
                    "type": "function",
                    "function": ["name": call.name, "arguments": call.arguments] as [String: Any],
                ]
            }
        }
        return AgentResult(assistantMessage: assistantMsg, toolCalls: agentCalls)
    }
}

// MARK: - OpenRouter client

/// Stateless networking client for the OpenRouter streaming chat-completions API.
enum AgentOpenRouterClient {

    private enum NetworkConstants {
        static let openRouterURL = URL(string: "https://openrouter.ai/api/v1/chat/completions")!
        static let requestTimeout: TimeInterval = 60
        static let maxErrorBodyBytes: Int = 512
    }

    /// Shared decoder. Streaming SSE chunks call into `OpenRouterUsagePayload`
    /// decoding once per usage-bearing chunk — over a long agent run with
    /// multiple turns and a streaming model that fires usage chunks
    /// per-segment, that adds up. Reentrant for `decode` after construction.
    nonisolated(unsafe) private static let usageDecoder = JSONDecoder()

    /// Streams a chat-completion request and returns the accumulated result.
    ///
    /// - Parameters:
    ///   - messages: The full raw message array (system + history + current user turn).
    ///   - tools: The tool schema array to include in the request.
    ///   - apiKey: A valid OpenRouter API key.
    ///   - model: The model identifier string (e.g. `"openai/gpt-4o"`).
    ///   - onPartialContent: Called on the `@MainActor` with each incremental
    ///     assistant text chunk as the stream arrives.
    /// - Returns: The fully accumulated `AgentResult` once the stream ends.
    /// - Throws: `AgentError` on HTTP failure or a malformed response, or
    ///   `CancellationError` if the enclosing task is cancelled.
    @MainActor
    static func streamCompletion(
        messages: [[String: Any]],
        tools: [[String: Any]],
        apiKey: String,
        model: String,
        feature: String = CostFeature.agentChat,
        onPartialContent: (String) -> Void
    ) async throws -> AgentResult {
        var request = URLRequest(url: NetworkConstants.openRouterURL)
        request.httpMethod = "POST"
        request.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization")
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.timeoutInterval = NetworkConstants.requestTimeout

        let body: [String: Any] = [
            "model": model,
            "messages": messages,
            "tools": tools,
            "stream": true,
            "stream_options": ["include_usage": true],
            "usage": ["include": true],
        ]
        let bodyData = try JSONSerialization.data(withJSONObject: body)
        request.httpBody = bodyData
        let requestPayloadJSON = String(data: bodyData, encoding: .utf8)

        let start = Date()
        let (bytes, response) = try await URLSession.shared.bytes(for: request)

        guard let http = response as? HTTPURLResponse else {
            throw AgentError.malformedResponse
        }
        guard (200..<300).contains(http.statusCode) else {
            // Collect the error body so the developer (and user) sees a helpful
            // message instead of a bare status code. OpenRouter returns JSON of
            // the form {"error":{"message":"…","code":…}}; we try to extract
            // that human-readable message and fall back to the raw body snippet.
            var bodyChunks: [String] = []
            for try await line in bytes.lines {
                bodyChunks.append(line)
                if bodyChunks.joined().count > NetworkConstants.maxErrorBodyBytes { break }
            }
            let rawBody = bodyChunks.joined()
            let detail = extractErrorMessage(from: rawBody, statusCode: http.statusCode)
            throw AgentError.httpError(detail)
        }

        var accumulator = StreamAccumulator()
        var capturedUsage: OpenRouterUsagePayload?
        var capturedModel: String = model

        for try await line in bytes.lines {
            guard line.hasPrefix("data: ") else { continue }
            let payload = String(line.dropFirst(6))
            guard payload != "[DONE]" else { break }
            guard
                let data = payload.data(using: .utf8),
                let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
            else { continue }

            if let modelField = json["model"] as? String, !modelField.isEmpty {
                capturedModel = modelField
            }

            // Usage-only chunk (choices is empty or absent) — capture and continue.
            if let usageRaw = json["usage"],
               (json["choices"] as? [[String: Any]])?.isEmpty != false {
                let usageData = try? JSONSerialization.data(withJSONObject: usageRaw)
                capturedUsage = usageData.flatMap { try? Self.usageDecoder.decode(OpenRouterUsagePayload.self, from: $0) }
                continue
            }

            guard
                let choices = json["choices"] as? [[String: Any]],
                let first = choices.first,
                let delta = first["delta"] as? [String: Any]
            else { continue }

            accumulator.applyDelta(delta)
            if !accumulator.content.isEmpty {
                onPartialContent(accumulator.content)
            }
        }

        let latencyMs = Int(Date().timeIntervalSince(start) * 1000)
        let baseResult = accumulator.toResult()
        let tokensUsed: AgentTokenUsage? = capturedUsage.map { usage in
            AgentTokenUsage(
                promptTokens: usage.prompt_tokens ?? 0,
                completionTokens: usage.completion_tokens ?? 0,
                cachedTokens: usage.prompt_tokens_details?.cached_tokens
            )
        }
        let result = AgentResult(
            assistantMessage: baseResult.assistantMessage,
            toolCalls: baseResult.toolCalls,
            tokensUsed: tokensUsed
        )
        let preview = (result.assistantMessage["content"] as? String)?.isEmpty == false
            ? result.assistantMessage["content"] as? String
            : "tool_calls: \(result.toolCalls.map(\.name).joined(separator: ", "))"
        CostLedger.shared.log(
            feature: feature,
            model: capturedModel,
            usage: capturedUsage,
            latencyMs: latencyMs,
            requestPayloadJSON: requestPayloadJSON,
            responseContentPreview: preview
        )

        return result
    }

    /// Extracts a human-readable error message from an API error body.
    ///
    /// OpenRouter (and many OpenAI-compatible APIs) return errors as:
    /// `{"error":{"message":"…","code":…}}` or `{"error":"…"}`.
    /// Falls back to a trimmed body snippet or a bare status-code string when
    /// the body cannot be parsed or is empty.
    static func extractErrorMessage(from body: String, statusCode: Int) -> String {
        let trimmed = body.trimmed
        guard !trimmed.isEmpty,
              let data = trimmed.data(using: .utf8),
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else {
            // No parseable JSON — surface a brief raw snippet if available.
            let snippet = trimmed.prefix(80)
            return snippet.isEmpty
                ? "HTTP \(statusCode)"
                : "HTTP \(statusCode): \(snippet)"
        }

        // {"error": {"message": "…"}}
        if let errorObj = json["error"] as? [String: Any],
           let message = errorObj["message"] as? String {
            return "HTTP \(statusCode): \(message)"
        }
        // {"error": "…"}
        if let message = json["error"] as? String {
            return "HTTP \(statusCode): \(message)"
        }
        // {"message": "…"}
        if let message = json["message"] as? String {
            return "HTTP \(statusCode): \(message)"
        }

        // JSON present but no recognised shape — fall back to snippet.
        let snippet = trimmed.prefix(80)
        return "HTTP \(statusCode): \(snippet)"
    }
}

// MARK: - Supporting types

struct AgentToolCall: Sendable {
    let id: String
    let name: String
    let arguments: String
}

struct AgentResult: @unchecked Sendable {
    let assistantMessage: [String: Any]
    let toolCalls: [AgentToolCall]
    let tokensUsed: AgentTokenUsage?

    init(
        assistantMessage: [String: Any],
        toolCalls: [AgentToolCall],
        tokensUsed: AgentTokenUsage? = nil
    ) {
        self.assistantMessage = assistantMessage
        self.toolCalls = toolCalls
        self.tokensUsed = tokensUsed
    }
}

enum AgentError: LocalizedError {
    case httpError(String)
    case malformedResponse

    var errorDescription: String? {
        switch self {
        case .httpError(let detail): detail
        case .malformedResponse: "Malformed response from API"
        }
    }
}
