import Foundation
import os.log

// MARK: - Catalog service

struct OpenRouterModelCatalogService: Sendable {

    private static let logger = Logger.app("OpenRouterModelCatalogService")
    private static let decoder = JSONDecoder()

    private enum Constants {
        static let openRouterModelsURL = "https://openrouter.ai/api/v1/models"
        static let modelsDevURL = "https://models.dev/api.json"
        static let xTitleHeader = "Podcastr"
        static let openRouterTimeout: TimeInterval = 30
        static let modelsDevTimeout: TimeInterval = 15
    }

    func fetchModels() async throws -> [OpenRouterModelOption] {
        async let openRouter = fetchOpenRouterModels()
        async let modelsDev = fetchModelsDevCatalogOptional()
        async let ollama = fetchOllamaModelsOptional()

        let models = try await openRouter
        let metadata = await modelsDev
        let ollamaModels = await ollama

        let openRouterOptions = models
            .map { OpenRouterModelOption(openRouter: $0, modelsDev: metadata) }

        return (openRouterOptions + ollamaModels.map(OpenRouterModelOption.init(ollama:)))
            .sorted { lhs, rhs in
                if lhs.isCompatible != rhs.isCompatible { return lhs.isCompatible && !rhs.isCompatible }
                if lhs.provider != rhs.provider { return lhs.provider == .openRouter }
                if lhs.createdAt != rhs.createdAt { return (lhs.createdAt ?? .distantPast) > (rhs.createdAt ?? .distantPast) }
                return lhs.name.localizedCaseInsensitiveCompare(rhs.name) == .orderedAscending
            }
    }

    private func fetchOpenRouterModels() async throws -> [ORModel] {
        guard let url = URL(string: Constants.openRouterModelsURL) else {
            throw CatalogError.decoding("Invalid OpenRouter URL")
        }
        var request = URLRequest(url: url)
        request.setValue(Constants.xTitleHeader, forHTTPHeaderField: "X-Title")
        request.timeoutInterval = Constants.openRouterTimeout

        let (data, _) = try await URLSession.shared.data(for: request)
        do {
            return try Self.decoder.decode(ORModelsResponse.self, from: data).data
        } catch {
            throw CatalogError.decoding("OpenRouter models: \(error.localizedDescription)")
        }
    }

    private func fetchModelsDevCatalogOptional() async -> ModelsDevCatalog? {
        do {
            guard let url = URL(string: Constants.modelsDevURL) else { return nil }
            var request = URLRequest(url: url)
            request.cachePolicy = .reloadRevalidatingCacheData
            request.timeoutInterval = Constants.modelsDevTimeout
            let (data, _) = try await URLSession.shared.data(for: request)
            let providers = try Self.decoder.decode([String: ModelsDevProvider].self, from: data)
            return ModelsDevCatalog(providers: providers)
        } catch {
            Self.logger.warning("models.dev metadata fetch failed (non-fatal): \(error, privacy: .public)")
            return nil
        }
    }

    private func fetchOllamaModelsOptional() async -> [OllamaTagModel] {
        do {
            return try await OllamaModelCatalogService().fetchModels()
        } catch {
            Self.logger.warning("Ollama model catalog fetch failed (non-fatal): \(error, privacy: .public)")
            return []
        }
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
    /// Lower-cased haystack used by the model selector's search filter.
    /// Stored (not computed) because the selector recomputes
    /// `visibleModels` on every keystroke, and a 200-model OpenRouter
    /// catalogue was rebuilding 200 joined strings + 200 lowercased
    /// copies per keystroke. Built once at init.
    let searchText: String

    init(openRouter model: ORModel, modelsDev: ModelsDevCatalog?) {
        let devModel = modelsDev?.openRouterModel(id: model.id)
        let pID = Self.providerID(from: model.id)
        let provider = modelsDev?.provider(id: pID)
        let supported = Set(model.supportedParameters ?? [])
        let input = model.architecture?.inputModalities ?? devModel?.modalities?.input ?? []
        let output = model.architecture?.outputModalities ?? devModel?.modalities?.output ?? []

        self.provider = .openRouter
        self.id = LLMModelReference(provider: .openRouter, modelID: model.id).storedID
        self.name = model.name
        self.providerID = pID
        self.providerName = Self.providerName(from: model.name, provider: provider, providerID: pID)
        self.providerIconURL = provider?.icon.flatMap { URL(string: $0) }
        self.modelDescription = model.description
        self.promptCostPerMillion = model.pricing?.prompt?.costPerMillion ?? devModel?.cost?.input
        self.completionCostPerMillion = model.pricing?.completion?.costPerMillion ?? devModel?.cost?.output
        self.cacheReadCostPerMillion = model.pricing?.inputCacheRead?.costPerMillion ?? devModel?.cost?.cacheRead
        self.cacheWriteCostPerMillion = model.pricing?.inputCacheWrite?.costPerMillion ?? devModel?.cost?.cacheWrite
        self.requestCost = model.pricing?.request.flatMap(Double.init)
        self.imageCost = model.pricing?.image.flatMap(Double.init)
        self.webSearchCost = model.pricing?.webSearch.flatMap(Double.init)
        self.contextLength = model.contextLength ?? model.topProvider?.contextLength ?? devModel?.limit?.context
        self.outputLimit = model.topProvider?.maxCompletionTokens ?? devModel?.limit?.output
        self.inputModalities = input
        self.outputModalities = output
        self.tokenizer = model.architecture?.tokenizer
        self.supportsTools = supported.contains("tools") || devModel?.toolCall == true
        self.supportsReasoning = supported.contains { $0.contains("reasoning") } || devModel?.reasoning == true
        self.supportsStructuredOutputs = supported.contains("structured_outputs") || devModel?.structuredOutput == true
        self.supportsResponseFormat = supported.contains("response_format") || self.supportsStructuredOutputs
        self.openWeights = devModel?.openWeights == true
        self.isModerated = model.topProvider?.isModerated
        self.createdAt = model.created.map { Date(timeIntervalSince1970: TimeInterval($0)) }
        self.knowledgeCutoff = model.knowledgeCutoff ?? devModel?.knowledge
        self.releaseDate = devModel?.releaseDate
        self.lastUpdated = devModel?.lastUpdated
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

    init(ollama model: OllamaTagModel) {
        let rawID = model.model ?? model.name
        let families = model.details?.families ?? []
        let family = model.details?.family ?? families.first ?? "ollama"
        let detailParts = [
            model.details?.parameterSize,
            model.details?.quantizationLevel,
        ].compactMap { $0 }.joined(separator: ", ")

        self.provider = .ollama
        self.id = LLMModelReference(provider: .ollama, modelID: rawID).storedID
        self.name = model.name
        self.providerID = "ollama-cloud"
        self.providerName = LLMProvider.ollama.displayName
        self.providerIconURL = nil
        self.modelDescription = detailParts.isEmpty
            ? "Cloud model available through Ollama's hosted API."
            : "Cloud model available through Ollama's hosted API. \(detailParts)."
        self.promptCostPerMillion = nil
        self.completionCostPerMillion = nil
        self.cacheReadCostPerMillion = nil
        self.cacheWriteCostPerMillion = nil
        self.requestCost = nil
        self.imageCost = nil
        self.webSearchCost = nil
        self.contextLength = nil
        self.outputLimit = nil
        self.inputModalities = ["text"]
        self.outputModalities = ["text"]
        self.tokenizer = family
        self.supportsTools = true
        self.supportsReasoning = rawID.localizedCaseInsensitiveContains("gpt-oss")
            || rawID.localizedCaseInsensitiveContains("qwen")
        self.supportsStructuredOutputs = true
        self.supportsResponseFormat = true
        self.openWeights = true
        self.isModerated = nil
        self.createdAt = model.modifiedAt
        self.knowledgeCutoff = nil
        self.releaseDate = nil
        self.lastUpdated = model.modifiedAt?.formatted(date: .abbreviated, time: .omitted)
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

    private static func providerID(from modelID: String) -> String {
        modelID.split(separator: "/", maxSplits: 1).first.map(String.init) ?? "openrouter"
    }

    private static func providerName(from modelName: String, provider: ModelsDevProvider?, providerID: String) -> String {
        if let provider { return provider.name }
        if let colon = modelName.firstIndex(of: ":") { return String(modelName[..<colon]) }
        return providerID
            .replacingOccurrences(of: "-", with: " ")
            .split(separator: " ").map { $0.capitalized }.joined(separator: " ")
    }
}

// DTOs and the `String.costPerMillion` helper live in `OpenRouterCatalogDTOs.swift`.
