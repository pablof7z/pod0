import Foundation

// MARK: - OpenRouter API DTOs

struct ORModelsResponse: Decodable, Sendable { var data: [ORModel] }

struct ORModel: Decodable, Sendable {
    var id: String
    var name: String
    var created: Int?
    var description: String?
    var contextLength: Int?
    var architecture: ORArchitecture?
    var pricing: ORPricing?
    var topProvider: ORTopProvider?
    var supportedParameters: [String]?
    var knowledgeCutoff: String?

    enum CodingKeys: String, CodingKey {
        case id, name, created, description
        case contextLength = "context_length"
        case architecture, pricing
        case topProvider = "top_provider"
        case supportedParameters = "supported_parameters"
        case knowledgeCutoff = "knowledge_cutoff"
    }
}

struct ORArchitecture: Decodable, Sendable {
    var inputModalities: [String]?
    var outputModalities: [String]?
    var tokenizer: String?

    enum CodingKeys: String, CodingKey {
        case inputModalities = "input_modalities"
        case outputModalities = "output_modalities"
        case tokenizer
    }
}

struct ORPricing: Decodable, Sendable {
    var prompt: String?
    var completion: String?
    var request: String?
    var image: String?
    var webSearch: String?
    var inputCacheRead: String?
    var inputCacheWrite: String?

    enum CodingKeys: String, CodingKey {
        case prompt, completion, request, image
        case webSearch = "web_search"
        case inputCacheRead = "input_cache_read"
        case inputCacheWrite = "input_cache_write"
    }
}

struct ORTopProvider: Decodable, Sendable {
    var contextLength: Int?
    var maxCompletionTokens: Int?
    var isModerated: Bool?

    enum CodingKeys: String, CodingKey {
        case contextLength = "context_length"
        case maxCompletionTokens = "max_completion_tokens"
        case isModerated = "is_moderated"
    }
}

// MARK: - models.dev DTOs

struct ModelsDevCatalog: Sendable {
    var providers: [String: ModelsDevProvider]
    func provider(id: String) -> ModelsDevProvider? { providers[id] }
    func openRouterModel(id: String) -> ModelsDevModel? { providers["openrouter"]?.models[id] }
}

struct ModelsDevProvider: Decodable, Hashable, Sendable {
    var id: String
    var name: String
    var icon: String?
    var models: [String: ModelsDevModel]
}

struct ModelsDevModel: Decodable, Hashable, Sendable {
    var id: String
    var name: String
    var reasoning: Bool?
    var toolCall: Bool?
    var structuredOutput: Bool?
    var knowledge: String?
    var releaseDate: String?
    var lastUpdated: String?
    var modalities: ModelsDevModalities?
    var openWeights: Bool?
    var cost: ModelsDevCost?
    var limit: ModelsDevLimit?

    enum CodingKeys: String, CodingKey {
        case id, name, reasoning
        case toolCall = "tool_call"
        case structuredOutput = "structured_output"
        case knowledge
        case releaseDate = "release_date"
        case lastUpdated = "last_updated"
        case modalities
        case openWeights = "open_weights"
        case cost, limit
    }
}

struct ModelsDevModalities: Decodable, Hashable, Sendable {
    var input: [String]?
    var output: [String]?
}

struct ModelsDevCost: Decodable, Hashable, Sendable {
    var input: Double?
    var output: Double?
    var cacheRead: Double?
    var cacheWrite: Double?

    enum CodingKeys: String, CodingKey {
        case input, output
        case cacheRead = "cache_read"
        case cacheWrite = "cache_write"
    }
}

struct ModelsDevLimit: Decodable, Hashable, Sendable {
    var context: Int?
    var output: Int?
}

// MARK: - Helpers

extension String {
    /// OpenRouter pricing values are per-token strings; convert to USD per
    /// million tokens for display. Returns `nil` for negative or non-numeric.
    var costPerMillion: Double? {
        guard let value = Double(self), value >= 0 else { return nil }
        return value * 1_000_000
    }
}
