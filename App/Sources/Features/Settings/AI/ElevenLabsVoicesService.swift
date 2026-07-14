import Foundation

struct ElevenLabsVoice: Identifiable, Hashable, Sendable {
    var id: String { voiceID }
    let voiceID: String
    let name: String
    let category: String
    let labels: [String: String]
    let previewURL: URL?

    var gender: String? { labels["gender"] }
    var accent: String? { labels["accent"] }
    var useCase: String? { labels["use_case"] }
    var descriptionLabel: String? { labels["description"] }
    var age: String? { labels["age"] }

    var pillLabels: [String] {
        var result: [String] = []
        if let gender, !gender.isEmpty { result.append(gender.capitalized) }
        if let accent, !accent.isEmpty { result.append(accent.capitalized) }
        if let useCase, !useCase.isEmpty {
            result.append(useCase.replacingOccurrences(of: "_", with: " ").capitalized)
        }
        return result
    }

    var searchText: String {
        var parts: [String] = [name, voiceID, category]
        parts.append(contentsOf: labels.values)
        return parts.joined(separator: " ").lowercased()
    }
}

enum ElevenLabsVoicesError: LocalizedError {
    case missingAPIKey
    case invalidResponse
    case unauthorized
    case server(Int)
    case decoding(String)
    case transport(String)

    var errorDescription: String? {
        switch self {
        case .missingAPIKey:    return "No ElevenLabs API key found."
        case .invalidResponse:  return "Unexpected response from ElevenLabs."
        case .unauthorized:     return "ElevenLabs rejected the API key."
        case .server(let code): return "ElevenLabs error (HTTP \(code))."
        case .decoding(let m):  return "Could not decode voices: \(m)"
        case .transport(let m): return m
        }
    }
}

struct ElevenLabsVoicesService: Sendable {

    private static let decoder = JSONDecoder()

    private enum Constants {
        static let voicesURL = "https://api.elevenlabs.io/v1/voices"
        static let apiKeyHeader = "xi-api-key"
        static let acceptHeader = "Accept"
        static let acceptValue = "application/json"
        static let timeout: TimeInterval = 30
    }

    func fetchVoices(apiKey: String) async throws -> [ElevenLabsVoice] {
        guard let url = URL(string: Constants.voicesURL) else {
            throw ElevenLabsVoicesError.invalidResponse
        }
        var request = URLRequest(url: url)
        request.setValue(apiKey, forHTTPHeaderField: Constants.apiKeyHeader)
        request.setValue(Constants.acceptValue, forHTTPHeaderField: Constants.acceptHeader)
        request.timeoutInterval = Constants.timeout

        let data: Data
        let response: URLResponse
        do {
            (data, response) = try await URLSession.shared.data(for: request)
        } catch {
            throw ElevenLabsVoicesError.transport(error.localizedDescription)
        }

        guard let http = response as? HTTPURLResponse else {
            throw ElevenLabsVoicesError.invalidResponse
        }
        switch http.statusCode {
        case 200..<300: break
        case 401, 403:  throw ElevenLabsVoicesError.unauthorized
        default:        throw ElevenLabsVoicesError.server(http.statusCode)
        }

        do {
            let decoded = try Self.decoder.decode(VoicesResponseDTO.self, from: data)
            return decoded.voices.map { dto in
                ElevenLabsVoice(
                    voiceID: dto.voice_id,
                    name: dto.name,
                    category: dto.category ?? "other",
                    labels: dto.labels ?? [:],
                    previewURL: dto.preview_url.flatMap { URL(string: $0) }
                )
            }
        } catch {
            throw ElevenLabsVoicesError.decoding(error.localizedDescription)
        }
    }

    // MARK: - DTOs

    private struct VoicesResponseDTO: Decodable {
        let voices: [VoiceDTO]
    }

    private struct VoiceDTO: Decodable {
        let voice_id: String
        let name: String
        let category: String?
        let labels: [String: String]?
        let preview_url: String?
    }
}

enum ElevenLabsVoiceCategoryOrder {
    static func sortKey(_ category: String) -> Int {
        switch category.lowercased() {
        case "premade":   return 0
        case "cloned":    return 1
        case "generated": return 2
        case "professional": return 3
        default:          return 4
        }
    }

    static func display(_ category: String) -> String {
        switch category.lowercased() {
        case "premade":      return "Premade"
        case "cloned":       return "Cloned"
        case "generated":    return "Generated"
        case "professional": return "Professional"
        default:             return category.capitalized
        }
    }
}
