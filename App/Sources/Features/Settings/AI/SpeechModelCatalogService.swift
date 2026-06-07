import Foundation

struct SpeechModelOption: Hashable, Sendable, Decodable {
    let id: String
    let label: String
}

struct SpeechModelCatalog: Equatable, Sendable {
    var elevenLabsSTT: [SpeechModelOption] = []
    var openRouterWhisper: [SpeechModelOption] = []
    var assemblyAISTT: [SpeechModelOption] = []
    var elevenLabsTTS: [SpeechModelOption] = []
}

extension SpeechModelCatalog: Decodable {
    enum CodingKeys: String, CodingKey {
        case elevenLabsSTT = "eleven_labs_stt"
        case openRouterWhisper = "open_router_whisper"
        case assemblyAISTT = "assembly_ai_stt"
        case elevenLabsTTS = "eleven_labs_tts"
    }
}

enum SpeechModelCatalogError: LocalizedError {
    case kernelUnavailable
    case invalidResponse
    case decoding(String)

    var errorDescription: String? {
        switch self {
        case .kernelUnavailable:
            return "App backend is not ready yet. Try again in a moment."
        case .invalidResponse:
            return "Unexpected speech model catalog response."
        case .decoding(let message):
            return message
        }
    }
}

struct SpeechModelCatalogService: Sendable {
    private static let decoder = JSONDecoder()

    func fetchCatalog() async throws -> SpeechModelCatalog {
        guard let handleBits = await MainActor.run(body: {
            KernelModel.shared?.podcastHandlePointer.map { Int(bitPattern: $0) }
        }) else {
            throw SpeechModelCatalogError.kernelUnavailable
        }

        let responseJSON = await Task.detached(priority: .userInitiated) {
            guard let handle = UnsafeMutableRawPointer(bitPattern: handleBits) else {
                return #"{"error":"Kernel handle unavailable"}"#
            }
            guard let ptr = nmp_app_podcast_speech_model_catalog(handle) else {
                return #"{"error":"null response from Rust"}"#
            }
            defer { nmp_app_free_string(ptr) }
            return String(cString: ptr)
        }.value

        guard let data = responseJSON.data(using: .utf8) else {
            throw SpeechModelCatalogError.invalidResponse
        }
        do {
            let envelope = try Self.decoder.decode(SpeechModelCatalogEnvelope.self, from: data)
            if let error = envelope.error {
                throw SpeechModelCatalogError.decoding(error)
            }
            guard let result = envelope.result else {
                throw SpeechModelCatalogError.invalidResponse
            }
            return result
        } catch let error as SpeechModelCatalogError {
            throw error
        } catch {
            throw SpeechModelCatalogError.decoding(error.localizedDescription)
        }
    }

    private struct SpeechModelCatalogEnvelope: Decodable {
        let result: SpeechModelCatalog?
        let error: String?
    }
}
