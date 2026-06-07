import Foundation

// MARK: - Catalog service

struct OpenRouterModelCatalogService: Sendable {
    private static let decoder = JSONDecoder()

    func fetchModels() async throws -> [OpenRouterModelOption] {
        guard let handleBits = await MainActor.run(body: {
            KernelModel.shared?.podcastHandlePointer.map { Int(bitPattern: $0) }
        }) else {
            throw CatalogError.decoding("Kernel handle unavailable")
        }

        let responseJSON = await Task.detached(priority: .userInitiated) {
            guard let handle = UnsafeMutableRawPointer(bitPattern: handleBits) else {
                return #"{"error":"Kernel handle unavailable"}"#
            }
            guard let ptr = nmp_app_podcast_provider_model_catalog(handle) else {
                return #"{"error":"null response from Rust"}"#
            }
            defer { nmp_app_free_string(ptr) }
            return String(cString: ptr)
        }.value

        guard let responseData = responseJSON.data(using: .utf8) else {
            throw CatalogError.decoding("Invalid provider catalog response")
        }
        let envelope = try Self.decoder.decode(ProviderModelCatalogEnvelope.self, from: responseData)
        if let error = envelope.error {
            throw CatalogError.decoding(error)
        }
        guard let result = envelope.result else {
            throw CatalogError.decoding("Provider catalog response missing result")
        }
        return result.models.map(OpenRouterModelOption.init(remote:))
    }
}

enum CatalogError: LocalizedError {
    case decoding(String)
    var errorDescription: String? {
        switch self { case .decoding(let msg): return msg }
    }
}

// MARK: - Public model type

struct OpenRouterModelOption: Identifiable, Hashable, Sendable {
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
    var createdAt: Date?
    var knowledgeCutoff: String?
    var releaseDate: String?
    var lastUpdated: String?
    let searchText: String

    init(remote model: ProviderModelOptionDTO) {
        self.provider = model.provider
        self.id = model.id
        self.name = model.name
        self.providerID = model.providerID
        self.providerName = model.providerName
        self.providerIconURL = model.providerIconURL
        self.modelDescription = model.modelDescription
        self.promptCostPerMillion = model.promptCostPerMillion
        self.completionCostPerMillion = model.completionCostPerMillion
        self.cacheReadCostPerMillion = model.cacheReadCostPerMillion
        self.cacheWriteCostPerMillion = model.cacheWriteCostPerMillion
        self.requestCost = model.requestCost
        self.imageCost = model.imageCost
        self.webSearchCost = model.webSearchCost
        self.contextLength = model.contextLength
        self.outputLimit = model.outputLimit
        self.inputModalities = model.inputModalities
        self.outputModalities = model.outputModalities
        self.tokenizer = model.tokenizer
        self.supportsTools = model.supportsTools
        self.supportsReasoning = model.supportsReasoning
        self.supportsResponseFormat = model.supportsResponseFormat
        self.supportsStructuredOutputs = model.supportsStructuredOutputs
        self.openWeights = model.openWeights
        self.isModerated = model.isModerated
        self.createdAt = model.createdAtEpochSecs.map {
            Date(timeIntervalSince1970: TimeInterval($0))
        }
        self.knowledgeCutoff = model.knowledgeCutoff
        self.releaseDate = model.releaseDate
        self.lastUpdated = model.lastUpdated
        self.searchText = model.searchText
    }

    /// A downloaded on-device model, surfaced in the per-role selector as the
    /// "Local" provider alongside OpenRouter/Ollama.
    init(local spec: LocalModelSpec) {
        self.provider = .local
        self.id = LLMModelReference(provider: .local, modelID: spec.id).storedID
        self.name = spec.displayName
        self.providerID = "local"
        self.providerName = LLMProvider.local.displayName
        self.providerIconURL = nil
        self.modelDescription = spec.description
        self.promptCostPerMillion = 0
        self.completionCostPerMillion = 0
        self.cacheReadCostPerMillion = nil
        self.cacheWriteCostPerMillion = nil
        self.requestCost = nil
        self.imageCost = nil
        self.webSearchCost = nil
        self.contextLength = nil
        self.outputLimit = nil
        self.inputModalities = ["text"]
        self.outputModalities = ["text"]
        self.tokenizer = "gemma"
        self.supportsTools = true
        self.supportsReasoning = false
        self.supportsStructuredOutputs = true
        self.supportsResponseFormat = true
        self.openWeights = true
        self.isModerated = nil
        self.createdAt = nil
        self.knowledgeCutoff = nil
        self.releaseDate = nil
        self.lastUpdated = nil
        self.searchText = Self.makeSearchText(
            id: self.id,
            name: self.name,
            providerName: self.providerName,
            providerID: self.providerID,
            modelDescription: self.modelDescription,
            tokenizer: self.tokenizer,
            inputModalities: self.inputModalities,
            outputModalities: self.outputModalities
        )
    }

    private static func makeSearchText(
        id: String,
        name: String,
        providerName: String,
        providerID: String,
        modelDescription: String?,
        tokenizer: String?,
        inputModalities: [String],
        outputModalities: [String]
    ) -> String {
        [id, name, providerName, providerID,
         modelDescription ?? "", tokenizer ?? "",
         inputModalities.joined(separator: " "),
         outputModalities.joined(separator: " ")]
        .joined(separator: " ").lowercased()
    }

    var isFree: Bool {
        promptCostPerMillion == 0 && completionCostPerMillion == 0
    }

    var isTextOutput: Bool {
        outputModalities.isEmpty || outputModalities.contains("text")
    }

    var isCompatible: Bool {
        isTextOutput && supportsResponseFormat
    }

    var compactPricing: String {
        guard let input = promptCostPerMillion, let output = completionCostPerMillion else { return "Variable" }
        if input == 0 && output == 0 { return "Free" }
        return "\(Self.money(input)) in / \(Self.money(output)) out"
    }

    static func money(_ value: Double) -> String {
        if value == 0 { return "$0" }
        if value < 0.01 { return String(format: "$%.4f", value) }
        if value < 1 { return String(format: "$%.2f", value) }
        if value.rounded() == value { return String(format: "$%.0f", value) }
        return String(format: "$%.2f", value)
    }

    static func perToken(_ value: Double?) -> String {
        guard let value else { return "Variable" }
        let token = value / 1_000_000
        if token == 0 { return "$0/token" }
        return String(format: "$%.9f/token", token)
    }
}
