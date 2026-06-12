import Foundation
import os.log

// MARK: - OpenRouterWhisperClient

/// Thin adapter for shared Rust-owned OpenRouter Whisper transcription.
///
/// Swift supplies the typed audio-source intent and converts the normalized
/// Rust response into the app's `Transcript` domain model. Rust owns OpenRouter
/// credentials, selected model lookup, request headers, multipart upload,
/// remote-audio staging, provider status handling, and response parsing.
actor OpenRouterWhisperClient {

    enum WhisperError: Swift.Error, LocalizedError, Sendable {
        case missingAPIKey
        case invalidAudioURL
        case downloadFailed(String)
        case kernelUnavailable
        case invalidResponse
        case http(status: Int, body: String?)
        case decoding(String)
        case cancelled
        case timedOut

        var errorDescription: String? {
            switch self {
            case .missingAPIKey:
                return "Add an OpenRouter API key in Settings → Intelligence → Providers to transcribe with Whisper."
            case .invalidAudioURL:
                return "Couldn't find the episode audio to transcribe."
            case .downloadFailed(let msg):
                return "Couldn't download audio for transcription: \(msg)"
            case .kernelUnavailable:
                return "Transcription backend is unavailable. Restart the app and try again."
            case .invalidResponse:
                return "OpenRouter returned an unexpected response. Try again in a moment."
            case .http(let status, _) where status == 401 || status == 403:
                return "OpenRouter rejected your API key. Update it in Settings → Intelligence → Providers."
            case .http(let status, _) where status == 429:
                return "OpenRouter rate-limited the request. Wait a minute and retry."
            case .http(let status, _) where status >= 500:
                return "OpenRouter is having trouble (\(status)). Retry in a few minutes."
            case .http(let status, _):
                return "OpenRouter returned an error (\(status))."
            case .decoding:
                return "OpenRouter returned a transcript shape we couldn't read."
            case .cancelled:
                return "Transcription cancelled."
            case .timedOut:
                return "Transcription took too long. Try again."
            }
        }
    }

    private static let logger = Logger.app("OpenRouterWhisperClient")
    private static let encoder = JSONEncoder()
    private static let decoder = JSONDecoder()

    // MARK: - API

    func transcribe(audioURL: URL, episodeID: UUID, languageHint: String? = nil) async throws -> Transcript {
        try Task.checkCancellation()
        let raw = try await transcribeViaRust(audioURL: audioURL, languageHint: languageHint)
        try Task.checkCancellation()

        Task { @MainActor in
            CostLedger.shared.logSTT(
                feature: CostFeature.sttOpenRouterWhisper,
                model: raw.model ?? "openai/whisper-1",
                costUSD: 0,
                audioDurationSeconds: raw.duration,
                latencyMs: raw.latencyMs ?? 0
            )
        }

        return Transcript.fromWhisperResponse(raw, episodeID: episodeID)
    }

    // MARK: - Private

    private func transcribeViaRust(audioURL: URL, languageHint: String?) async throws -> WhisperVerboseResponse {
        guard let handleBits = await MainActor.run(body: {
            KernelModel.shared?.podcastHandlePointer.map { Int(bitPattern: $0) }
        }) else {
            throw WhisperError.kernelUnavailable
        }

        let intent = OpenRouterWhisperIntent(
            audioURL: audioURL.absoluteString,
            languageHint: languageHint?.isEmpty == false ? languageHint : nil
        )
        let requestData = try Self.encoder.encode(intent)
        guard let requestJSON = String(data: requestData, encoding: .utf8) else {
            throw WhisperError.decoding("Could not encode transcription request.")
        }

        Self.logger.info("submitting Whisper request through Rust provider transport")
        let responseJSON = await Task.detached(priority: .userInitiated) {
            guard let handle = UnsafeMutableRawPointer(bitPattern: handleBits) else {
                return #"{"error":{"kind":"store_unavailable","message":"Kernel handle unavailable"}}"#
            }
            return requestJSON.withCString { cRequest in
                guard let ptr = nmp_app_podcast_openrouter_whisper_transcribe(handle, cRequest) else {
                    return #"{"error":{"kind":"store_unavailable","message":"null response from Rust"}}"#
                }
                defer { nmp_free_string(ptr) }
                return String(cString: ptr)
            }
        }.value

        guard let responseData = responseJSON.data(using: .utf8) else {
            throw WhisperError.invalidResponse
        }
        do {
            let envelope = try Self.decoder.decode(OpenRouterWhisperEnvelope.self, from: responseData)
            if let error = envelope.error {
                throw Self.whisperError(from: error)
            }
            guard let result = envelope.result else {
                throw WhisperError.invalidResponse
            }
            return result
        } catch let error as WhisperError {
            throw error
        } catch {
            Self.logger.error("Whisper FFI decode failed: \(String(describing: error), privacy: .public)")
            throw WhisperError.decoding("Could not decode transcription response: \(error)")
        }
    }

    private static func whisperError(from error: OpenRouterWhisperBackendError) -> WhisperError {
        switch error.kind {
        case "missing_api_key":
            return .missingAPIKey
        case "invalid_audio_url":
            return .invalidAudioURL
        case "download_failed":
            return .downloadFailed(error.message ?? "Unknown download error.")
        case "timed_out":
            return .timedOut
        case "invalid_key":
            return .http(status: error.statusCode ?? 401, body: error.message)
        case "rate_limited":
            return .http(status: error.statusCode ?? 429, body: error.message)
        case "server_error":
            return .http(status: error.statusCode ?? 500, body: error.message)
        case "decoding_error":
            return .decoding(error.message ?? "Could not decode transcription response.")
        case "store_unavailable":
            return .kernelUnavailable
        default:
            return .http(status: error.statusCode ?? 500, body: error.message)
        }
    }
}

// MARK: - DTOs

private struct OpenRouterWhisperIntent: Encodable {
    let audioURL: String
    let languageHint: String?

    enum CodingKeys: String, CodingKey {
        case audioURL = "audio_url"
        case languageHint = "language_hint"
    }
}

private struct OpenRouterWhisperEnvelope: Decodable {
    var result: WhisperVerboseResponse?
    var error: OpenRouterWhisperBackendError?
}

private struct OpenRouterWhisperBackendError: Decodable {
    var kind: String
    var message: String?
    var statusCode: Int?

    enum CodingKeys: String, CodingKey {
        case kind, message
        case statusCode = "status_code"
    }
}

struct WhisperVerboseResponse: Codable, Sendable {
    let task: String?
    let language: String?
    let duration: Double?
    let text: String?
    let segments: [WhisperSegment]?
    let model: String?
    let latencyMs: Int?

    enum CodingKeys: String, CodingKey {
        case task, language, duration, text, segments, model
        case latencyMs = "latency_ms"
    }
}

struct WhisperSegment: Codable, Sendable {
    let id: Int?
    let start: Double
    let end: Double
    let text: String
}

// MARK: - Transcript adapter

extension Transcript {
    static func fromWhisperResponse(_ raw: WhisperVerboseResponse, episodeID: UUID) -> Transcript {
        let language = raw.language ?? "en"
        let segments = (raw.segments ?? []).map { seg in
            Segment(
                start: seg.start,
                end: seg.end,
                text: seg.text.trimmingCharacters(in: .whitespacesAndNewlines)
            )
        }
        return Transcript(
            episodeID: episodeID,
            language: language,
            source: .whisper,
            segments: segments,
            speakers: [],
            generatedAt: Date()
        )
    }
}
