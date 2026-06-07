import Foundation
import os.log

struct ElevenLabsSynthesizedAudio: Sendable {
    let data: Data
    let contentType: String
    let model: String
    let voiceID: String
    let latencyMs: Int?
}

enum ElevenLabsTTSBackendError: LocalizedError, Sendable {
    case missingAPIKey
    case missingVoiceID
    case invalidRequest(String)
    case emptyAudio
    case kernelUnavailable
    case http(status: Int, message: String?)
    case network(String)
    case decoding(String)

    var errorDescription: String? {
        switch self {
        case .missingAPIKey:
            return "No ElevenLabs API key. Connect first."
        case .missingVoiceID:
            return "No voice selected. Pick a voice first."
        case .invalidRequest(let message):
            return message
        case .emptyAudio:
            return "ElevenLabs returned no audio."
        case .kernelUnavailable:
            return "App backend is not ready yet. Try again in a moment."
        case .http(let status, _) where status == 401 || status == 403:
            return "ElevenLabs rejected the API key."
        case .http(let status, _) where status == 429:
            return "ElevenLabs rate-limited the request. Wait a minute and retry."
        case .http(let status, _) where status >= 500:
            return "ElevenLabs is having trouble (\(status)). Retry in a few minutes."
        case .http(let status, _):
            return "ElevenLabs error (HTTP \(status))."
        case .network(let message):
            return message
        case .decoding(let message):
            return "Could not decode TTS response: \(message)"
        }
    }
}

struct ElevenLabsTTSBackendClient: Sendable {
    private static let logger = Logger.app("ElevenLabsTTSBackendClient")
    private static let encoder = JSONEncoder()
    private static let decoder = JSONDecoder()

    func synthesize(
        text: String,
        voiceID: String,
        model: String? = nil
    ) async throws -> ElevenLabsSynthesizedAudio {
        guard !voiceID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            throw ElevenLabsTTSBackendError.missingVoiceID
        }
        guard let handleBits = await MainActor.run(body: {
            KernelModel.shared?.podcastHandlePointer.map { Int(bitPattern: $0) }
        }) else {
            throw ElevenLabsTTSBackendError.kernelUnavailable
        }

        let intent = TTSIntent(text: text, voiceID: voiceID, model: model)
        let requestData = try Self.encoder.encode(intent)
        guard let requestJSON = String(data: requestData, encoding: .utf8) else {
            throw ElevenLabsTTSBackendError.decoding("Could not encode TTS request.")
        }

        let responseJSON = await Task.detached(priority: .userInitiated) {
            guard let handle = UnsafeMutableRawPointer(bitPattern: handleBits) else {
                return #"{"error":{"kind":"store_unavailable","message":"Kernel handle unavailable"}}"#
            }
            return requestJSON.withCString { cRequest in
                guard let ptr = nmp_app_podcast_elevenlabs_tts_synthesize(handle, cRequest) else {
                    return #"{"error":{"kind":"store_unavailable","message":"null response from Rust"}}"#
                }
                defer { nmp_app_free_string(ptr) }
                return String(cString: ptr)
            }
        }.value

        guard let responseData = responseJSON.data(using: .utf8) else {
            throw ElevenLabsTTSBackendError.decoding("Invalid UTF-8 response.")
        }
        do {
            let envelope = try Self.decoder.decode(TTSEnvelope.self, from: responseData)
            if let error = envelope.error {
                throw Self.ttsError(from: error)
            }
            guard let result = envelope.result else {
                throw ElevenLabsTTSBackendError.decoding("Response missing result.")
            }
            guard let audio = Data(base64Encoded: result.audioBase64), !audio.isEmpty else {
                throw ElevenLabsTTSBackendError.emptyAudio
            }
            return ElevenLabsSynthesizedAudio(
                data: audio,
                contentType: result.contentType,
                model: result.model,
                voiceID: result.voiceID,
                latencyMs: result.latencyMs
            )
        } catch let error as ElevenLabsTTSBackendError {
            throw error
        } catch {
            Self.logger.error("TTS FFI decode failed: \(String(describing: error), privacy: .public)")
            throw ElevenLabsTTSBackendError.decoding(error.localizedDescription)
        }
    }

    private static func ttsError(from error: TTSBackendError) -> ElevenLabsTTSBackendError {
        switch error.kind {
        case "missing_api_key":
            return .missingAPIKey
        case "invalid_request":
            if error.message?.contains("voice_id") == true {
                return .missingVoiceID
            }
            return .invalidRequest(error.message ?? "Invalid ElevenLabs TTS request.")
        case "empty_audio":
            return .emptyAudio
        case "invalid_key":
            return .http(status: error.statusCode ?? 401, message: error.message)
        case "rate_limited":
            return .http(status: error.statusCode ?? 429, message: error.message)
        case "server_error":
            return .http(status: error.statusCode ?? 500, message: error.message)
        case "network_error":
            return .network(error.message ?? "Could not reach ElevenLabs.")
        case "store_unavailable":
            return .kernelUnavailable
        default:
            return .http(status: error.statusCode ?? 500, message: error.message)
        }
    }
}

private struct TTSIntent: Encodable {
    let text: String
    let voiceID: String
    let model: String?

    enum CodingKeys: String, CodingKey {
        case text
        case voiceID = "voice_id"
        case model
    }
}

private struct TTSEnvelope: Decodable {
    let result: TTSResult?
    let error: TTSBackendError?
}

private struct TTSResult: Decodable {
    let audioBase64: String
    let contentType: String
    let model: String
    let voiceID: String
    let latencyMs: Int?

    enum CodingKeys: String, CodingKey {
        case audioBase64 = "audio_base64"
        case contentType = "content_type"
        case model
        case voiceID = "voice_id"
        case latencyMs = "latency_ms"
    }
}

private struct TTSBackendError: Decodable {
    let kind: String
    let message: String?
    let statusCode: Int?

    enum CodingKeys: String, CodingKey {
        case kind, message
        case statusCode = "status_code"
    }
}
