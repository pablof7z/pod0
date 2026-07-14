import Foundation

/// Generates a short title for a chat conversation via the user-configured
/// `memoryCompilationModel`. Designed to run as a fire-and-forget background
/// task after the first assistant text reply lands.
///
/// Reuses `WikiOpenRouterClient` because it already forces the assistant to
/// reply in JSON via `response_format: { "type": "json_object" }`, which lets
/// us parse a `{"title": "..."}` envelope without prompt-jail breaks.
enum AgentChatTitleGenerator {

    static let maxTranscriptChars = 4_000
    static let maxTitleChars = 60

    /// Builds the small transcript snippet we feed the model. We keep only
    /// `.user` and `.assistant` text — tool batches and errors add noise
    /// without informing the conversation topic.
    static func buildTranscriptSnippet(from messages: [ChatMessage]) -> String {
        var lines: [String] = []
        for msg in messages {
            switch msg.role {
            case .user:
                lines.append("User: \(msg.text)")
            case .assistant:
                lines.append("Assistant: \(msg.text)")
            case .toolBatch, .error, .skillActivated:
                continue
            }
        }
        let joined = lines.joined(separator: "\n")
        if joined.count <= maxTranscriptChars { return joined }
        return String(joined.prefix(maxTranscriptChars))
    }

    /// Returns the generated title, or `nil` if generation failed (missing
    /// credential, network error, unparseable response). Callers should treat
    /// a nil return as "leave the conversation untitled and try again later".
    static func generate(transcript: String, model: String) async -> String? {
        let trimmed = transcript.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        let reference = LLMModelReference(storedID: model)
        guard LLMProviderCredentialResolver.hasAPIKey(for: reference.provider) else {
            return nil
        }
        let client = WikiOpenRouterClient.live(model: model)
        do {
            let json = try await client.compile(
                systemPrompt: systemPrompt,
                userPrompt: userPrompt(transcript: trimmed),
                feature: CostFeature.agentChatTitle
            )
            return parseTitle(from: json)
        } catch {
            return nil
        }
    }

    static let systemPrompt: String = """
    You write very short titles for chat-conversation history lists. Reply
    strictly with JSON of the form {"title": String}. The title must be 2 to
    6 words, no punctuation at the end, no quotation marks, no emoji, and must
    describe the actual subject of the conversation (not "Chat" or "Untitled").
    """

    static func userPrompt(transcript: String) -> String {
        """
        Generate a short title that summarises what this conversation is about.

        Transcript:
        \(transcript)
        """
    }

    static func parseTitle(from json: String) -> String? {
        guard let data = json.data(using: .utf8),
              let root = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let raw = root["title"] as? String else { return nil }
        let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
            .trimmingCharacters(in: CharacterSet(charactersIn: "\"'.,;:!?"))
        guard !trimmed.isEmpty else { return nil }
        if trimmed.count <= maxTitleChars { return trimmed }
        return String(trimmed.prefix(maxTitleChars))
    }
}
