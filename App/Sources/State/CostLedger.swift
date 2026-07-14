import Combine
import Foundation

enum CostFeature {
    static let agentChat = "agent.chat"
    static let agentChatTitle = "agent.chat.title"
    static let agentNostr = "agent.nostr"
    static let episodeSummary = "episode.summary"
    static let briefingCompose = "briefing.compose"
    static let wikiCompile = "wiki.compile"
    static let embeddingsOpenRouter = "embeddings.openrouter"
    static let embeddingsOllama = "embeddings.ollama"
    static let categorizationRecompute = "categorization.recompute"
    /// AssemblyAI pre-recorded transcription. Cost reported in the poll
    /// response's `usage.cost` field.
    static let sttAssemblyAI = "stt.assemblyai"
    /// ElevenLabs Scribe transcription. No cost field in the response; we log
    /// `costUSD = 0` and rely on the user's ElevenLabs dashboard for billing.
    static let sttScribe = "stt.scribe"
    /// OpenRouter Whisper transcription. Cost field would arrive via OpenRouter
    /// usage but the current text-only Whisper response doesn't expose it.
    /// Records `costUSD = 0` and the audio duration from the verbose response.
    static let sttOpenRouterWhisper = "stt.openrouter.whisper"

    static func displayName(for feature: String) -> String {
        switch feature {
        case agentChat:              return "Agent chat"
        case agentChatTitle:         return "Agent chat title"
        case agentNostr:             return "Agent (Nostr)"
        case episodeSummary:         return "Episode summary"
        case briefingCompose:        return "Briefing"
        case wikiCompile:            return "Wiki compile"
        case embeddingsOpenRouter:   return "Embeddings (OpenRouter)"
        case embeddingsOllama:       return "Embeddings (Ollama)"
        case categorizationRecompute: return "Categorization"
        case sttAssemblyAI:          return "STT (AssemblyAI)"
        case sttScribe:              return "STT (Scribe)"
        case sttOpenRouterWhisper:   return "STT (Whisper)"
        default:                     return feature
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
    var requestPayloadJSON: String?
    var responseContentPreview: String?
    /// Audio duration (seconds) for STT records. Nil for token-shaped records
    /// (LLM calls, embeddings). Codable-back-compat via `decodeIfPresent`.
    var audioDurationSeconds: Double?

    private enum CodingKeys: String, CodingKey {
        case id, at, feature, model
        case promptTokens, completionTokens, cachedTokens, reasoningTokens
        case costUSD, latencyMs
        case requestPayloadJSON, responseContentPreview
        case audioDurationSeconds
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        self.id = try c.decode(UUID.self, forKey: .id)
        self.at = try c.decode(Date.self, forKey: .at)
        self.feature = try c.decode(String.self, forKey: .feature)
        self.model = try c.decode(String.self, forKey: .model)
        self.promptTokens = try c.decode(Int.self, forKey: .promptTokens)
        self.completionTokens = try c.decode(Int.self, forKey: .completionTokens)
        self.cachedTokens = try c.decode(Int.self, forKey: .cachedTokens)
        self.reasoningTokens = try c.decode(Int.self, forKey: .reasoningTokens)
        self.costUSD = try c.decode(Double.self, forKey: .costUSD)
        self.latencyMs = try c.decode(Int.self, forKey: .latencyMs)
        self.requestPayloadJSON = try c.decodeIfPresent(String.self, forKey: .requestPayloadJSON)
        self.responseContentPreview = try c.decodeIfPresent(String.self, forKey: .responseContentPreview)
        self.audioDurationSeconds = try c.decodeIfPresent(Double.self, forKey: .audioDurationSeconds)
    }

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
        requestPayloadJSON: String? = nil,
        responseContentPreview: String? = nil,
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
        self.requestPayloadJSON = requestPayloadJSON
        self.responseContentPreview = responseContentPreview
        self.audioDurationSeconds = audioDurationSeconds
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

@MainActor
final class CostLedger: ObservableObject {
    static let shared = CostLedger()

    @Published private(set) var records: [UsageRecord]

    private let directoryURL: URL
    private let fileURL: URL

    private init() {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
            ?? FileManager.default.temporaryDirectory
        directoryURL = base.appendingPathComponent("UsageLedger", isDirectory: true)
        fileURL = directoryURL.appendingPathComponent("ledger.json")
        try? FileManager.default.createDirectory(at: directoryURL, withIntermediateDirectories: true)
        records = Self.load(from: fileURL)
    }

    func log(
        feature: String,
        model: String,
        usage: OpenRouterUsagePayload?,
        latencyMs: Int,
        requestPayloadJSON: String? = nil,
        responseContentPreview: String? = nil
    ) {
        let record = UsageRecord(
            id: UUID(),
            at: Date(),
            feature: feature,
            model: model,
            promptTokens: usage?.prompt_tokens ?? 0,
            completionTokens: usage?.completion_tokens ?? 0,
            cachedTokens: usage?.prompt_tokens_details?.cached_tokens ?? 0,
            reasoningTokens: usage?.completion_tokens_details?.reasoning_tokens ?? 0,
            costUSD: usage?.cost ?? 0,
            latencyMs: latencyMs,
            requestPayloadJSON: requestPayloadJSON,
            responseContentPreview: responseContentPreview
        )
        records.insert(record, at: 0)
        save()
    }

    func logOllama(
        feature: String,
        model: String,
        promptTokens: Int,
        completionTokens: Int,
        latencyMs: Int,
        requestPayloadJSON: String? = nil,
        responseContentPreview: String? = nil
    ) {
        let record = UsageRecord(
            id: UUID(),
            at: Date(),
            feature: feature,
            model: model,
            promptTokens: promptTokens,
            completionTokens: completionTokens,
            cachedTokens: 0,
            reasoningTokens: 0,
            costUSD: 0,
            latencyMs: latencyMs,
            requestPayloadJSON: requestPayloadJSON,
            responseContentPreview: responseContentPreview
        )
        records.insert(record, at: 0)
        save()
    }

    /// STT-shaped record: audio duration in seconds + optional cost. Use this
    /// from `AssemblyAITranscriptClient`, `ElevenLabsScribeClient`, and
    /// `OpenRouterWhisperClient` to record transcription activity. Cost may be
    /// zero when the provider's response doesn't surface it — the entry still
    /// appears in the Usage view so the user has a unified activity log.
    func logSTT(
        feature: String,
        model: String,
        costUSD: Double,
        audioDurationSeconds: Double?,
        latencyMs: Int,
        promptTokens: Int = 0,
        completionTokens: Int = 0,
        requestPayloadJSON: String? = nil,
        responseContentPreview: String? = nil
    ) {
        let record = UsageRecord(
            id: UUID(),
            at: Date(),
            feature: feature,
            model: model,
            promptTokens: promptTokens,
            completionTokens: completionTokens,
            cachedTokens: 0,
            reasoningTokens: 0,
            costUSD: costUSD,
            latencyMs: latencyMs,
            requestPayloadJSON: requestPayloadJSON,
            responseContentPreview: responseContentPreview,
            audioDurationSeconds: audioDurationSeconds
        )
        records.insert(record, at: 0)
        save()
    }

    func clear() {
        records = []
        save()
    }

    private func save() {
        guard let data = try? Self.encoder.encode(records) else { return }
        try? data.write(to: fileURL, options: [.atomic])
    }

    /// Configured once. `save()` runs on every cost-log (every LLM
    /// call the user makes), so per-call encoder construction +
    /// `.iso8601` / `.sortedKeys` configuration was real (if modest)
    /// waste. Matches the same fix applied to `AgentRunLogger.save()`.
    private static let encoder: JSONEncoder = {
        let e = JSONEncoder()
        e.dateEncodingStrategy = .iso8601
        e.outputFormatting = [.sortedKeys]
        return e
    }()

    private static func load(from url: URL) -> [UsageRecord] {
        guard let data = try? Data(contentsOf: url) else { return [] }
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return (try? decoder.decode([UsageRecord].self, from: data)) ?? []
    }
}
