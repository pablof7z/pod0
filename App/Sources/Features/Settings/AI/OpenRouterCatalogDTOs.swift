import Foundation

struct ProviderModelCatalogEnvelope: Decodable, Sendable {
    var result: ProviderModelCatalogResult?
    var error: String?
}

struct ProviderModelCatalogResult: Decodable, Sendable {
    var models: [ProviderModelOptionDTO]
}

struct ProviderModelOptionDTO: Decodable, Sendable {
    var provider: LLMProvider
    var id: String
    var name: String
    var providerID: String
    var providerName: String
    var providerIconURL: URL?
    var modelDescription: String?
    var promptCostPerMillion: Double?
    var completionCostPerMillion: Double?
    var cacheReadCostPerMillion: Double?
    var cacheWriteCostPerMillion: Double?
    var requestCost: Double?
    var imageCost: Double?
    var webSearchCost: Double?
    var contextLength: Int?
    var outputLimit: Int?
    var inputModalities: [String]
    var outputModalities: [String]
    var tokenizer: String?
    var supportsTools: Bool
    var supportsReasoning: Bool
    var supportsResponseFormat: Bool
    var supportsStructuredOutputs: Bool
    var openWeights: Bool
    var isModerated: Bool?
    var createdAtEpochSecs: Double?
    var knowledgeCutoff: String?
    var releaseDate: String?
    var lastUpdated: String?
    var searchText: String

    enum CodingKeys: String, CodingKey {
        case provider, id, name
        case providerID = "provider_id"
        case providerName = "provider_name"
        case providerIconURL = "provider_icon_url"
        case modelDescription = "model_description"
        case promptCostPerMillion = "prompt_cost_per_million"
        case completionCostPerMillion = "completion_cost_per_million"
        case cacheReadCostPerMillion = "cache_read_cost_per_million"
        case cacheWriteCostPerMillion = "cache_write_cost_per_million"
        case requestCost = "request_cost"
        case imageCost = "image_cost"
        case webSearchCost = "web_search_cost"
        case contextLength = "context_length"
        case outputLimit = "output_limit"
        case inputModalities = "input_modalities"
        case outputModalities = "output_modalities"
        case tokenizer
        case supportsTools = "supports_tools"
        case supportsReasoning = "supports_reasoning"
        case supportsResponseFormat = "supports_response_format"
        case supportsStructuredOutputs = "supports_structured_outputs"
        case openWeights = "open_weights"
        case isModerated = "is_moderated"
        case createdAtEpochSecs = "created_at_epoch_secs"
        case knowledgeCutoff = "knowledge_cutoff"
        case releaseDate = "release_date"
        case lastUpdated = "last_updated"
        case searchText = "search_text"
    }
}
