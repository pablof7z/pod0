import Foundation

enum AgentOllamaClient {
    private enum NetworkConstants {
        static let chatURL = URL(string: "https://ollama.com/api/chat")!
        static let requestTimeout: TimeInterval = 60
        static let maxErrorBodyBytes: Int = 512
    }

    @MainActor
    static func streamCompletion(
        messages: [[String: Any]],
        tools: [[String: Any]],
        apiKey: String,
        model: String,
        feature: String = CostFeature.agentChat,
        chatURL: URL? = nil,
        onPartialContent: (String) -> Void
    ) async throws -> AgentResult {
        let resolvedURL = chatURL ?? NetworkConstants.chatURL
        var request = URLRequest(url: resolvedURL)
        request.httpMethod = "POST"
        request.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization")
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.timeoutInterval = NetworkConstants.requestTimeout

        let body: [String: Any] = [
            "model": model,
            "messages": ollamaMessages(from: messages),
            "tools": tools,
            "stream": true,
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
            var bodyChunks: [String] = []
            for try await line in bytes.lines {
                bodyChunks.append(line)
                if bodyChunks.joined().count > NetworkConstants.maxErrorBodyBytes { break }
            }
            let detail = AgentOpenRouterClient.extractErrorMessage(
                from: bodyChunks.joined(),
                statusCode: http.statusCode
            )
            throw AgentError.httpError(detail)
        }

        var content = ""
        var toolCalls: [[String: Any]] = []
        var promptTokens = 0
        var completionTokens = 0
        var capturedModel = model

        for try await line in bytes.lines {
            guard !line.isBlank,
                  let data = line.data(using: .utf8),
                  let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
            else { continue }

            if let message = json["message"] as? [String: Any] {
                if let chunk = message["content"] as? String {
                    content += chunk
                    if !content.isEmpty { onPartialContent(content) }
                }
                if let calls = message["tool_calls"] as? [[String: Any]] {
                    toolCalls.append(contentsOf: calls)
                }
            }
            if (json["done"] as? Bool) == true {
                promptTokens = (json["prompt_eval_count"] as? Int) ?? 0
                completionTokens = (json["eval_count"] as? Int) ?? 0
                capturedModel = (json["model"] as? String) ?? model
                break
            }
        }

        let latencyMs = Int(Date().timeIntervalSince(start) * 1000)
        let baseResult = result(content: content, ollamaToolCalls: toolCalls)
        let agentResult = AgentResult(
            assistantMessage: baseResult.assistantMessage,
            toolCalls: baseResult.toolCalls,
            tokensUsed: AgentTokenUsage(
                promptTokens: promptTokens,
                completionTokens: completionTokens,
                cachedTokens: nil
            )
        )
        let preview = content.isEmpty
            ? "tool_calls: \(agentResult.toolCalls.map(\.name).joined(separator: ", "))"
            : String(content.prefix(500))
        CostLedger.shared.logOllama(
            feature: feature,
            model: capturedModel,
            promptTokens: promptTokens,
            completionTokens: completionTokens,
            latencyMs: latencyMs,
            requestPayloadJSON: requestPayloadJSON,
            responseContentPreview: preview
        )

        return agentResult
    }

    private static func ollamaMessages(from raw: [[String: Any]]) -> [[String: Any]] {
        var result: [[String: Any]] = []
        var toolNamesByID: [String: String] = [:]

        for message in raw {
            let role = message["role"] as? String ?? "user"
            if role == "assistant" {
                var out: [String: Any] = ["role": "assistant"]
                if let content = message["content"] as? String, !content.isEmpty {
                    out["content"] = content
                }
                if let calls = message["tool_calls"] as? [[String: Any]] {
                    let converted = calls.compactMap { call -> [String: Any]? in
                        guard let function = call["function"] as? [String: Any],
                              let name = function["name"] as? String else { return nil }
                        if let id = call["id"] as? String { toolNamesByID[id] = name }
                        return [
                            "type": "function",
                            "function": [
                                "name": name,
                                "arguments": jsonObject(from: function["arguments"]),
                            ],
                        ]
                    }
                    if !converted.isEmpty { out["tool_calls"] = converted }
                }
                result.append(out)
            } else if role == "tool" {
                let callID = message["tool_call_id"] as? String ?? ""
                guard let toolName = toolNamesByID[callID] else { continue }
                result.append([
                    "role": "tool",
                    "tool_name": toolName,
                    "content": message["content"] as? String ?? "",
                ])
            } else {
                result.append([
                    "role": role,
                    "content": message["content"] as? String ?? "",
                ])
            }
        }
        return result
    }

    private static func result(content: String, ollamaToolCalls: [[String: Any]]) -> AgentResult {
        let calls: [AgentToolCall] = ollamaToolCalls.enumerated().compactMap { index, call in
            guard let function = call["function"] as? [String: Any],
                  let name = function["name"] as? String,
                  !name.isEmpty else { return nil }
            let args = argumentsString(from: function["arguments"])
            return AgentToolCall(id: "ollama_call_\(index)_\(UUID().uuidString)", name: name, arguments: args)
        }

        var assistant: [String: Any] = ["role": "assistant"]
        if !content.isEmpty { assistant["content"] = content }
        if !calls.isEmpty {
            assistant["tool_calls"] = calls.map { call in
                [
                    "id": call.id,
                    "type": "function",
                    "function": [
                        "name": call.name,
                        "arguments": call.arguments,
                    ],
                ] as [String: Any]
            }
        }
        return AgentResult(assistantMessage: assistant, toolCalls: calls)
    }

    private static func jsonObject(from value: Any?) -> Any {
        if let raw = value as? String,
           let data = raw.data(using: .utf8),
           let parsed = try? JSONSerialization.jsonObject(with: data) {
            return parsed
        }
        return value ?? [:]
    }

    private static func argumentsString(from value: Any?) -> String {
        if let string = value as? String { return string }
        guard let value,
              JSONSerialization.isValidJSONObject(value),
              let data = try? JSONSerialization.data(withJSONObject: value),
              let string = String(data: data, encoding: .utf8)
        else { return "{}" }
        return string
    }
}
