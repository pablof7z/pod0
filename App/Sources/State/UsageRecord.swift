import Foundation

enum CostFeature {
    static let agentChat = "agent.chat"
    static let agentChatTitle = "agent.chat.title"
    static let episodeSummary = "episode.summary"
    static let embeddingsOpenRouter = "embeddings.openrouter"
    static let embeddingsOllama = "embeddings.ollama"
    static let categorizationRecompute = "categorization.recompute"
    static let sttAssemblyAI = "stt.assemblyai"
    static let sttScribe = "stt.scribe"
    static let sttOpenRouterWhisper = "stt.openrouter.whisper"

    static func displayName(for feature: String) -> String {
        switch feature {
        case agentChat: "Agent chat"
        case agentChatTitle: "Agent chat title"
        case episodeSummary: "Episode summary"
        case embeddingsOpenRouter: "Embeddings (OpenRouter)"
        case embeddingsOllama: "Embeddings (Ollama)"
        case categorizationRecompute: "Categorization"
        case sttAssemblyAI: "STT (AssemblyAI)"
        case sttScribe: "STT (Scribe)"
        case sttOpenRouterWhisper: "STT (Whisper)"
        default: feature
        }
    }
}

struct UsageRecord: Codable, Hashable, Identifiable, Sendable {
    var id: UUID
    var at: Date
    var feature: String
    var model: String
    var promptTokens: Int
    var completionTokens: Int
    var cachedTokens: Int
    var reasoningTokens: Int
    var costUSD: Double
    var latencyMs: Int
    var audioDurationSeconds: Double?

    /// Decode-only legacy fields. Current logging APIs never accept content,
    /// and `CostLedger` clears these immediately after a successful decode.
    var requestPayloadJSON: String?
    var responseContentPreview: String?

    init(
        id: UUID,
        at: Date,
        feature: String,
        model: String,
        promptTokens: Int,
        completionTokens: Int,
        cachedTokens: Int,
        reasoningTokens: Int,
        costUSD: Double,
        latencyMs: Int,
        audioDurationSeconds: Double? = nil
    ) {
        self.id = id
        self.at = at
        self.feature = feature
        self.model = model
        self.promptTokens = promptTokens
        self.completionTokens = completionTokens
        self.cachedTokens = cachedTokens
        self.reasoningTokens = reasoningTokens
        self.costUSD = costUSD
        self.latencyMs = latencyMs
        self.audioDurationSeconds = audioDurationSeconds
        requestPayloadJSON = nil
        responseContentPreview = nil
    }

    var withoutContent: UsageRecord {
        var copy = self
        copy.requestPayloadJSON = nil
        copy.responseContentPreview = nil
        return copy
    }
}

struct OpenRouterUsagePayload: Decodable, Sendable {
    struct PromptDetails: Decodable, Sendable {
        let cached_tokens: Int?
        let cache_write_tokens: Int?
        let audio_tokens: Int?
    }

    struct CompletionDetails: Decodable, Sendable {
        let reasoning_tokens: Int?
    }

    let prompt_tokens: Int?
    let completion_tokens: Int?
    let total_tokens: Int?
    let cost: Double?
    let prompt_tokens_details: PromptDetails?
    let completion_tokens_details: CompletionDetails?
}
