import Foundation
import os.log

// MARK: - AssemblyAITranscriptClient

/// Thin adapter for shared Rust-owned AssemblyAI pre-recorded transcription.
///
/// Swift supplies only the typed audio-source intent and converts the
/// normalized Rust response into the app's `Transcript` domain model. Rust owns
/// AssemblyAI credentials, selected speech model fallback list, submit/poll
/// HTTP, provider status handling, and response parsing.
actor AssemblyAITranscriptClient {

    enum TranscribeError: Swift.Error, LocalizedError, Sendable {
        case missingAPIKey
        case invalidAudioURL
        case invalidResponse
        case kernelUnavailable
        case http(status: Int, body: String?)
        case decoding(String)
        case network(String)
        case remoteError(String)
        case cancelled
        case timedOut

        var errorDescription: String? {
            switch self {
            case .missingAPIKey:
                return "Add an AssemblyAI API key in Settings → Intelligence → Providers to transcribe episodes."
            case .invalidAudioURL:
                return "Couldn't find the episode audio to transcribe."
            case .invalidResponse:
                return "AssemblyAI returned an unexpected response. Try again in a moment."
            case .kernelUnavailable:
                return "Transcription backend is unavailable. Restart the app and try again."
            case .http(let status, _) where status == 401 || status == 403:
                return "AssemblyAI rejected your API key. Update it in Settings → Intelligence → Providers."
            case .http(let status, _) where status == 422:
                return "AssemblyAI couldn't process the audio (file format or URL not accepted)."
            case .http(let status, _) where status == 429:
                return "AssemblyAI rate-limited the request. Wait a minute and retry."
            case .http(let status, _) where status >= 500:
                return "AssemblyAI is having trouble (\(status)). Retry in a few minutes."
            case .http(let status, _):
                return "AssemblyAI returned an unexpected error (\(status))."
            case .decoding:
                return "AssemblyAI returned a transcript shape we couldn't read."
            case .network:
                return "Could not reach AssemblyAI. Check your connection and try again."
            case .remoteError(let message):
                return "AssemblyAI couldn't transcribe this episode: \(message)"
            case .cancelled:
                return "Transcription cancelled."
            case .timedOut:
                return "Transcription took too long. Try again - long episodes can take several minutes."
            }
        }
    }

    private static let logger = Logger.app("AssemblyAITranscriptClient")
    private static let encoder = JSONEncoder()
    private static let decoder = JSONDecoder()

    func submit(
        audioURL: URL,
        episodeID: UUID,
        speechModels: [String] = [],
        speakerLabels: Bool = true,
        languageDetection: Bool = true,
        languageHint: String? = nil
    ) async throws -> AssemblyAIJob {
        try Task.checkCancellation()
        _ = speakerLabels
        let hint = languageDetection ? nil : languageHint
        let payload = try await transcribeViaRust(audioURL: audioURL, languageHint: hint)
        try Task.checkCancellation()
        Task { @MainActor in
            CostLedger.shared.logSTT(
                feature: CostFeature.sttAssemblyAI,
                model: payload.model ?? "universal-3-pro,universal-2",
                costUSD: payload.usage?.cost ?? 0,
                audioDurationSeconds: payload.usage?.seconds ?? payload.audio_duration,
                latencyMs: payload.latencyMs ?? 0,
                promptTokens: payload.usage?.input_tokens ?? 0,
                completionTokens: payload.usage?.output_tokens ?? 0
            )
        }
        return AssemblyAIJob(
            transcriptID: payload.id ?? UUID().uuidString,
            episodeID: episodeID,
            createdAt: Date(),
            languageHint: hint,
            speechModels: speechModels,
            inlineResult: payload
        )
    }

    func pollResult(_ job: AssemblyAIJob) async throws -> Transcript {
        guard let payload = job.inlineResult else { throw TranscribeError.invalidResponse }
        return Transcript.fromAssemblyAI(payload, episodeID: job.episodeID, languageHint: job.languageHint)
    }

    private func transcribeViaRust(audioURL: URL, languageHint: String?) async throws -> AssemblyAITranscriptPayload {
        guard let handleBits = await MainActor.run(body: {
            KernelModel.shared?.podcastHandlePointer.map { Int(bitPattern: $0) }
        }) else {
            throw TranscribeError.kernelUnavailable
        }
        let intent = AssemblyAITranscriptIntent(
            audioURL: audioURL.absoluteString,
            languageHint: languageHint?.isEmpty == false ? languageHint : nil
        )
        let requestData = try Self.encoder.encode(intent)
        guard let requestJSON = String(data: requestData, encoding: .utf8) else {
            throw TranscribeError.decoding("Could not encode transcription request.")
        }
        Self.logger.info("submitting AssemblyAI request through Rust provider transport")
        let responseJSON = await Task.detached(priority: .userInitiated) {
            guard let handle = UnsafeMutableRawPointer(bitPattern: handleBits) else {
                return #"{"error":{"kind":"store_unavailable","message":"Kernel handle unavailable"}}"#
            }
            return requestJSON.withCString { cRequest in
                guard let ptr = nmp_app_podcast_assemblyai_transcribe(handle, cRequest) else {
                    return #"{"error":{"kind":"store_unavailable","message":"null response from Rust"}}"#
                }
                defer { nmp_free_string(ptr) }
                return String(cString: ptr)
            }
        }.value
        guard let responseData = responseJSON.data(using: .utf8) else {
            throw TranscribeError.invalidResponse
        }
        do {
            let envelope = try Self.decoder.decode(AssemblyAITranscriptEnvelope.self, from: responseData)
            if let error = envelope.error {
                throw Self.transcribeError(from: error)
            }
            guard let result = envelope.result else {
                throw TranscribeError.invalidResponse
            }
            return result
        } catch let error as TranscribeError {
            throw error
        } catch {
            Self.logger.error("AssemblyAI FFI decode failed: \(String(describing: error), privacy: .public)")
            throw TranscribeError.decoding("Could not decode transcription response: \(error)")
        }
    }

    static func transcribeError(from error: AssemblyAITranscriptBackendError) -> TranscribeError {
        switch error.kind {
        case "missing_api_key":
            return .missingAPIKey
        case "invalid_audio_url":
            return .invalidAudioURL
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
        case "network_error":
            return .network(error.message ?? "Network error.")
        case "remote_error":
            return .remoteError(error.message ?? "AssemblyAI transcription failed.")
        case "store_unavailable":
            return .kernelUnavailable
        default:
            return .http(status: error.statusCode ?? 500, body: error.message)
        }
    }
}
