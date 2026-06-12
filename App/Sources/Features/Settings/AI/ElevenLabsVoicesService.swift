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
    case kernelUnavailable
    case transport(String)

    var errorDescription: String? {
        switch self {
        case .missingAPIKey:    return "No ElevenLabs API key found."
        case .invalidResponse:  return "Unexpected response from ElevenLabs."
        case .unauthorized:     return "ElevenLabs rejected the API key."
        case .server(let code): return "ElevenLabs error (HTTP \(code))."
        case .decoding(let m):  return "Could not decode voices: \(m)"
        case .kernelUnavailable:return "App backend is not ready yet. Try again in a moment."
        case .transport(let m): return m
        }
    }
}

struct ElevenLabsVoicesService: Sendable {

    private static let decoder = JSONDecoder()

    func fetchVoices() async throws -> [ElevenLabsVoice] {
        guard let handleBits = await MainActor.run(body: {
            KernelModel.shared?.podcastHandlePointer.map { Int(bitPattern: $0) }
        }) else {
            throw ElevenLabsVoicesError.kernelUnavailable
        }

        let responseJSON = await Task.detached(priority: .userInitiated) {
            guard let handle = UnsafeMutableRawPointer(bitPattern: handleBits) else {
                return #"{"error":{"kind":"store_unavailable","message":"Kernel handle unavailable"}}"#
            }
            guard let ptr = nmp_app_podcast_elevenlabs_voice_catalog(handle) else {
                return #"{"error":{"kind":"store_unavailable","message":"null response from Rust"}}"#
            }
            defer { nmp_free_string(ptr) }
            return String(cString: ptr)
        }.value

        guard let data = responseJSON.data(using: .utf8) else {
            throw ElevenLabsVoicesError.invalidResponse
        }
        do {
            let envelope = try Self.decoder.decode(VoiceCatalogEnvelope.self, from: data)
            if let error = envelope.error {
                throw Self.voiceError(from: error)
            }
            guard let result = envelope.result else {
                throw ElevenLabsVoicesError.invalidResponse
            }
            return result.voices.map { dto in
                ElevenLabsVoice(
                    voiceID: dto.voice_id,
                    name: dto.name,
                    category: dto.category ?? "other",
                    labels: dto.labels ?? [:],
                    previewURL: dto.preview_url.flatMap { URL(string: $0) }
                )
            }
        } catch let error as ElevenLabsVoicesError {
            throw error
        } catch {
            throw ElevenLabsVoicesError.decoding(error.localizedDescription)
        }
    }

    // MARK: - DTOs

    private struct VoiceCatalogEnvelope: Decodable {
        let result: VoicesResponseDTO?
        let error: BackendErrorDTO?
    }

    private struct VoicesResponseDTO: Decodable {
        let provider: String?
        let voices: [VoiceDTO]
    }

    private struct VoiceDTO: Decodable {
        let voice_id: String
        let name: String
        let category: String?
        let labels: [String: String]?
        let preview_url: String?
    }

    private struct BackendErrorDTO: Decodable {
        let kind: String
        let message: String?
        let statusCode: Int?

        enum CodingKeys: String, CodingKey {
            case kind, message
            case statusCode = "status_code"
        }
    }

    private static func voiceError(from error: BackendErrorDTO) -> ElevenLabsVoicesError {
        switch error.kind {
        case "missing_api_key":
            return .missingAPIKey
        case "invalid_key":
            return .unauthorized
        case "server_error", "rate_limited":
            return .server(error.statusCode ?? 0)
        case "decoding_error":
            return .decoding(error.message ?? "Unexpected response from ElevenLabs.")
        case "store_unavailable":
            return .kernelUnavailable
        case "network_error":
            return .transport(error.message ?? "Could not reach ElevenLabs.")
        default:
            return .transport(error.message ?? "Unexpected ElevenLabs voice catalog error.")
        }
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
