import Foundation
import os.log

// MARK: - ElevenLabsScribeClient

/// Thin adapter for shared Rust-owned ElevenLabs Scribe transcription.
///
/// Swift supplies only the typed audio-source intent and converts the
/// normalized Rust response into the app's `Transcript` domain model. Rust owns
/// ElevenLabs credentials, selected Scribe model lookup, request headers,
/// local-file vs `source_url` multipart shaping, provider status handling, and
/// response parsing.
actor ElevenLabsScribeClient {

    enum ScribeError: Swift.Error, LocalizedError, Sendable {
        case missingAPIKey
        case invalidResponse
        case invalidAudioURL
        case kernelUnavailable
        case http(status: Int, body: String?)
        case decoding(String)
        case network(String)
        case cancelled
        case timedOut

        var errorDescription: String? {
            switch self {
            case .missingAPIKey:
                return "Add an ElevenLabs API key in Settings → Intelligence → Providers to transcribe episodes."
            case .invalidResponse:
                return "ElevenLabs returned an unexpected response. Try again in a moment."
            case .invalidAudioURL:
                return "Couldn't find the episode audio to transcribe."
            case .kernelUnavailable:
                return "Transcription backend is unavailable. Restart the app and try again."
            case .http(let status, _) where status == 401 || status == 403:
                return "ElevenLabs rejected your API key. Update it in Settings → Intelligence → Providers."
            case .http(let status, _) where status == 422:
                return "ElevenLabs couldn't process the audio (file format or URL not accepted)."
            case .http(let status, _) where status == 429:
                return "ElevenLabs rate-limited the request. Wait a minute and retry."
            case .http(let status, _) where status >= 500:
                return "ElevenLabs is having trouble (\(status)). Retry in a few minutes."
            case .http(let status, _):
                return "ElevenLabs returned an unexpected error (\(status))."
            case .decoding:
                return "ElevenLabs returned a transcript shape we couldn't read."
            case .network:
                return "Could not reach ElevenLabs. Check your connection and try again."
            case .cancelled:
                return "Transcription cancelled."
            case .timedOut:
                return "Transcription took too long. Try again - long episodes can take several minutes."
            }
        }
    }

    private static let logger = Logger.app("ElevenLabsScribeClient")
    private static let encoder = JSONEncoder()
    private static let decoder = JSONDecoder()

    // MARK: - API

    /// Submits an audio source for transcription. The shared Rust endpoint
    /// returns the full Scribe result inline; we preserve the old job wrapper
    /// so `TranscriptIngestService` keeps one lifecycle for submit/poll style
    /// STT providers.
    func submit(
        audioURL: URL,
        episodeID: UUID,
        languageHint: String? = nil
    ) async throws -> ScribeJob {
        try Task.checkCancellation()
        let raw = try await transcribeViaRust(audioURL: audioURL, languageHint: languageHint)
        try Task.checkCancellation()

        Task { @MainActor in
            CostLedger.shared.logSTT(
                feature: CostFeature.sttScribe,
                model: raw.model ?? "scribe_v1",
                costUSD: 0,
                audioDurationSeconds: raw.duration ?? raw.words?.last?.end,
                latencyMs: raw.latencyMs ?? 0
            )
        }

        return ScribeJob(
            requestID: UUID().uuidString,
            episodeID: episodeID,
            createdAt: Date(),
            languageHint: languageHint,
            inlineResult: raw
        )
    }

    func pollResult(_ job: ScribeJob) async throws -> Transcript {
        guard let raw = job.inlineResult else { throw ScribeError.invalidResponse }
        return Transcript.fromScribeRaw(raw, episodeID: job.episodeID, languageHint: job.languageHint)
    }

    // MARK: - Private

    private func transcribeViaRust(audioURL: URL, languageHint: String?) async throws -> ScribeRawResult {
        guard let handleBits = await MainActor.run(body: {
            KernelModel.shared?.podcastHandlePointer.map { Int(bitPattern: $0) }
        }) else {
            throw ScribeError.kernelUnavailable
        }

        let intent = ElevenLabsScribeIntent(
            audioURL: audioURL.absoluteString,
            languageHint: languageHint?.isEmpty == false ? languageHint : nil
        )
        let requestData = try Self.encoder.encode(intent)
        guard let requestJSON = String(data: requestData, encoding: .utf8) else {
            throw ScribeError.decoding("Could not encode transcription request.")
        }

        Self.logger.info("submitting Scribe request through Rust provider transport")
        let responseJSON = await Task.detached(priority: .userInitiated) {
            guard let handle = UnsafeMutableRawPointer(bitPattern: handleBits) else {
                return #"{"error":{"kind":"store_unavailable","message":"Kernel handle unavailable"}}"#
            }
            return requestJSON.withCString { cRequest in
                guard let ptr = nmp_app_podcast_elevenlabs_scribe_transcribe(handle, cRequest) else {
                    return #"{"error":{"kind":"store_unavailable","message":"null response from Rust"}}"#
                }
                defer { nmp_free_string(ptr) }
                return String(cString: ptr)
            }
        }.value

        guard let responseData = responseJSON.data(using: .utf8) else {
            throw ScribeError.invalidResponse
        }
        do {
            let envelope = try Self.decoder.decode(ElevenLabsScribeEnvelope.self, from: responseData)
            if let error = envelope.error {
                throw Self.scribeError(from: error)
            }
            guard let result = envelope.result else {
                throw ScribeError.invalidResponse
            }
            return result
        } catch let error as ScribeError {
            throw error
        } catch {
            Self.logger.error("Scribe FFI decode failed: \(String(describing: error), privacy: .public)")
            throw ScribeError.decoding("Could not decode transcription response: \(error)")
        }
    }

    static func scribeError(from error: ElevenLabsScribeBackendError) -> ScribeError {
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
        case "store_unavailable":
            return .kernelUnavailable
        default:
            return .http(status: error.statusCode ?? 500, body: error.message)
        }
    }
}

// MARK: - Transcript adapter

extension Transcript {
    /// Converts a Scribe raw result into our internal `Transcript`. Words of
    /// type `spacing` are dropped. Words of type `audio_event` (`[laughter]`,
    /// `[music]`) are folded into the body text in-place; agent/wiki surfaces
    /// can use them for context while the reader can hide them.
    static func fromScribeRaw(
        _ raw: ScribeRawResult,
        episodeID: UUID,
        languageHint: String?
    ) -> Transcript {
        let language = raw.language_code ?? languageHint ?? "en-US"
        let words = raw.words ?? []

        var speakers: [String: Speaker] = [:]
        var segments: [Segment] = []
        var bufferText = ""
        var bufferWords: [Word] = []
        var bufferStart: Double = 0
        var bufferEnd: Double = 0
        var bufferSpeaker: String?

        @inline(__always) func flush() {
            guard !bufferWords.isEmpty else { return }
            let speakerID: UUID?
            if let label = bufferSpeaker {
                if let existing = speakers[label] {
                    speakerID = existing.id
                } else {
                    let new = Speaker(label: label, displayName: nil)
                    speakers[label] = new
                    speakerID = new.id
                }
            } else {
                speakerID = nil
            }
            segments.append(
                Segment(
                    start: bufferStart,
                    end: bufferEnd,
                    speakerID: speakerID,
                    text: bufferText.trimmingCharacters(in: .whitespacesAndNewlines),
                    words: bufferWords
                )
            )
            bufferText = ""
            bufferWords = []
        }

        for w in words where w.type != "spacing" {
            let speakerSwitch = bufferSpeaker != nil && w.speaker_id != bufferSpeaker
            let pauseBoundary = !bufferWords.isEmpty && (w.start - bufferEnd) > 1.2
            if speakerSwitch || pauseBoundary {
                flush()
            }
            if bufferWords.isEmpty {
                bufferStart = w.start
                bufferSpeaker = w.speaker_id
            }
            bufferEnd = w.end
            if !bufferText.isEmpty { bufferText.append(" ") }
            bufferText.append(w.text)
            bufferWords.append(Word(start: w.start, end: w.end, text: w.text))
        }
        flush()

        return Transcript(
            episodeID: episodeID,
            language: language,
            source: .scribeV1,
            segments: segments,
            speakers: Array(speakers.values),
            generatedAt: Date()
        )
    }
}
