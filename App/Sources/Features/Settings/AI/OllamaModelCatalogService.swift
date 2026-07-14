import Foundation
import os.log

struct OllamaModelCatalogService: Sendable {
    private static let logger = Logger.app("OllamaModelCatalogService")
    private static let decoder = JSONDecoder()

    private enum Constants {
        static let tagsURL = "https://ollama.com/api/tags"
        static let timeout: TimeInterval = 30
    }

    private let apiKeyProvider: @Sendable () throws -> String?

    init(apiKeyProvider: @Sendable @escaping () throws -> String? = { try OllamaCredentialStore.apiKey() }) {
        self.apiKeyProvider = apiKeyProvider
    }

    func fetchModels() async throws -> [OllamaTagModel] {
        guard let url = URL(string: Constants.tagsURL) else {
            throw CatalogError.decoding("Invalid Ollama tags URL")
        }

        var request = URLRequest(url: url)
        if let apiKey = try apiKeyProvider(), !apiKey.isEmpty {
            request.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization")
        }
        request.timeoutInterval = Constants.timeout

        let (data, response) = try await URLSession.shared.data(for: request)
        guard let http = response as? HTTPURLResponse else {
            throw CatalogError.decoding("Ollama models: missing HTTP response")
        }
        guard (200..<300).contains(http.statusCode) else {
            throw CatalogError.decoding("Ollama models returned HTTP \(http.statusCode)")
        }

        do {
            return try Self.decoder.decode(OllamaTagsResponse.self, from: data).models
        } catch {
            Self.logger.warning("Ollama models decode failed: \(error, privacy: .public)")
            throw CatalogError.decoding("Ollama models: \(error.localizedDescription)")
        }
    }
}

struct OllamaTagsResponse: Decodable, Sendable {
    var models: [OllamaTagModel]
}

struct OllamaTagModel: Decodable, Sendable {
    var name: String
    var model: String?
    var modifiedAt: Date?
    var size: Int64?
    var digest: String?
    var details: Details?

    enum CodingKeys: String, CodingKey {
        case name, model, size, digest, details
        case modifiedAt = "modified_at"
    }

    struct Details: Decodable, Sendable {
        var family: String?
        var families: [String]?
        var parameterSize: String?
        var quantizationLevel: String?

        enum CodingKeys: String, CodingKey {
            case family, families
            case parameterSize = "parameter_size"
            case quantizationLevel = "quantization_level"
        }
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        name = try c.decode(String.self, forKey: .name)
        model = try c.decodeIfPresent(String.self, forKey: .model)
        size = try c.decodeIfPresent(Int64.self, forKey: .size)
        digest = try c.decodeIfPresent(String.self, forKey: .digest)
        details = try c.decodeIfPresent(Details.self, forKey: .details)
        if let raw = try c.decodeIfPresent(String.self, forKey: .modifiedAt) {
            modifiedAt = ISO8601DateFormatter.ollamaFlexible.date(from: raw)
                ?? ISO8601DateFormatter.ollamaInternet.date(from: raw)
        } else {
            modifiedAt = nil
        }
    }
}

private extension ISO8601DateFormatter {
    nonisolated(unsafe) static let ollamaInternet: ISO8601DateFormatter = {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime]
        return formatter
    }()

    nonisolated(unsafe) static let ollamaFlexible: ISO8601DateFormatter = {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return formatter
    }()
}
