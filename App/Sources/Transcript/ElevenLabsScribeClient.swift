import Foundation
import os.log

// MARK: - ElevenLabsScribeClient

/// Thin REST client for ElevenLabs Scribe batch transcription.
///
/// API per the published OpenAPI spec at `https://api.elevenlabs.io/openapi.json`:
///
///   `POST https://api.elevenlabs.io/v1/speech-to-text`
///
///   `multipart/form-data` body. Required fields:
///     • `model_id`        — `"scribe_v1"` or `"scribe_v2"` (enum, no others)
///     • Exactly ONE audio source:
///         - `file`        — binary file bytes (use when audio is on local disk)
///         - `source_url`  — HTTPS URL string (use when audio is remote — server fetches it)
///
///   Useful optional fields we set:
///     • `timestamps_granularity` — `"word"` (default already, set explicitly)
///     • `diarize`               — `"true"`
///     • `tag_audio_events`      — `"true"`
///     • `language_code`         — ISO-639-1 hint (omit for auto-detect)
///
///   Header: `xi-api-key: <key>`
///
///   The endpoint is **synchronous by default**: the response body for HTTP 200
///   is the full transcript JSON (`{ language_code, language_probability, text,
///   words: [...] }`). The async webhook path is only entered when the request
///   includes `webhook=true` — we do NOT set that, so we never see a 202.
///
/// Why this client used to never deliver a transcript:
///   1. It passed `episode.enclosureURL` (an HTTPS URL) into the multipart
///      `file` field, then read its bytes via `Data(contentsOf:)`. That call
///      synchronously downloads the entire MP3 (50–150 MB for an hour-long
///      episode) on the actor before the upload starts — and then races
///      against the default 60-second `URLRequest.timeoutInterval`, which
///      is far shorter than transcription wall time.
///   2. There is no documented polling endpoint in the synchronous flow, so
///      the old `pollResult` retry loop was unreachable in practice and the
///      whole `AsyncJobResponse` code path was a phantom contract.
///
/// The fix: keep the `submit` → `pollResult` shape (so `TranscriptionQueue`
/// keeps compiling) but make `submit` actually perform the synchronous request,
/// stash the inline result on the returned `ScribeJob`, and have `pollResult`
/// just return that inline result. Choose the multipart audio source based on
/// the URL scheme — `file://` → `file` field with bytes, `https://` →
/// `source_url` field with the URL string (the server fetches it for us).
actor ElevenLabsScribeClient {

    enum ScribeError: Swift.Error, LocalizedError, Sendable {
        case missingAPIKey
        case invalidResponse
        case invalidAudioURL
        case http(status: Int, body: String?)
        case decoding(String)
        case cancelled
        case timedOut

        /// User-facing copy. These messages land directly in the
        /// `TranscribingInProgressView` "Failed" panel via
        /// `TranscriptionQueue.failed(message:)` — without `LocalizedError`
        /// the user would see raw Swift case names like
        /// `http(status: 401, body: Optional("..."))`.
        var errorDescription: String? {
            switch self {
            case .missingAPIKey:
                return "Add an ElevenLabs API key in Settings → Intelligence → Providers to transcribe episodes."
            case .invalidResponse:
                return "ElevenLabs returned an unexpected response. Try again in a moment."
            case .invalidAudioURL:
                return "Couldn't find the episode audio to transcribe."
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
            case .cancelled:
                return "Transcription cancelled."
            case .timedOut:
                return "Transcription took too long. Try again — the second attempt usually completes faster."
            }
        }
    }

    private static let logger = Logger.app("ElevenLabsScribeClient")

    /// Shared decoder for the synchronous `/v1/speech-to-text`
    /// response. Reentrant for `decode` after construction; one per
    /// transcribed episode is plenty.
    nonisolated(unsafe) private static let decoder = JSONDecoder()

    /// 10 minutes — Scribe is synchronous and a 60-minute episode can take
    /// several minutes to transcribe server-side. The default URLRequest
    /// timeout of 60s would (and did, for every long episode) fire first
    /// and surface as `URLError.timedOut`.
    static let requestTimeout: TimeInterval = 600

    private let baseURL: URL
    private let session: URLSession
    private let modelID: String
    private let credential: @Sendable () throws -> String?

    init(
        baseURL: URL = URL(string: "https://api.elevenlabs.io")!,
        modelID: String = "scribe_v2",
        session: URLSession = .shared,
        credential: @escaping @Sendable () throws -> String? = { try ElevenLabsCredentialStore.apiKey() }
    ) {
        self.baseURL = baseURL
        self.modelID = modelID
        self.session = session
        self.credential = credential
    }

    // MARK: API

    /// Submits an audio source for transcription. The synchronous endpoint
    /// returns the full transcript inline; we stash it on the returned
    /// `ScribeJob` and `pollResult` just unwraps it.
    ///
    /// `audioURL` may be either:
    ///   • a `file://` URL — we read its bytes and POST as the `file` field
    ///   • an `https://` URL — we POST as the `source_url` field and let
    ///     ElevenLabs fetch it server-side (no client-side download)
    func submit(
        audioURL: URL,
        episodeID: UUID,
        languageHint: String? = nil
    ) async throws -> ScribeJob {
        try Task.checkCancellation()
        guard let key = try credential(), !key.isEmpty else { throw ScribeError.missingAPIKey }

        let endpoint = baseURL.appendingPathComponent("v1/speech-to-text")
        let boundary = "Boundary-\(UUID().uuidString)"
        var request = URLRequest(url: endpoint)
        request.httpMethod = "POST"
        request.setValue(key, forHTTPHeaderField: "xi-api-key")
        request.setValue("multipart/form-data; boundary=\(boundary)", forHTTPHeaderField: "Content-Type")
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        request.timeoutInterval = Self.requestTimeout

        let audioField = try Self.audioField(for: audioURL)
        let body = try Self.multipartBody(
            boundary: boundary,
            modelID: modelID,
            languageHint: languageHint,
            audio: audioField
        )

        try Task.checkCancellation()
        Self.logger.info(
            "submitting Scribe request — model=\(self.modelID, privacy: .public) source=\(audioField.kind, privacy: .public) bytes=\(body.count, privacy: .public)"
        )

        let submitStart = Date()
        let data: Data
        let response: URLResponse
        do {
            (data, response) = try await session.upload(for: request, from: body)
        } catch is CancellationError {
            throw ScribeError.cancelled
        } catch let error as URLError where error.code == .cancelled {
            throw ScribeError.cancelled
        } catch let error as URLError where error.code == .timedOut {
            throw ScribeError.timedOut
        }

        try Task.checkCancellation()
        try Self.assertOK(response: response, data: data)

        let raw: ScribeRawResult
        do {
            raw = try Self.decoder.decode(ScribeRawResult.self, from: data)
        } catch {
            let preview = String(data: data.prefix(500), encoding: .utf8) ?? "<binary>"
            Self.logger.error("Scribe decode failed: \(String(describing: error), privacy: .public) body=\(preview, privacy: .public)")
            throw ScribeError.decoding("Could not decode /speech-to-text response: \(error)")
        }

        // Scribe's response has no cost or `audio_duration` field. Approximate
        // duration from the last word's end timestamp (seconds). The user's
        // billing dashboard is the source of truth for cost; this record is
        // for activity tracking + duration visibility.
        let audioDuration = raw.words?.last?.end
        let latencyMs = Int(Date().timeIntervalSince(submitStart) * 1000)
        let modelLabel = self.modelID
        Task { @MainActor in
            CostLedger.shared.logSTT(
                feature: CostFeature.sttScribe,
                model: modelLabel,
                costUSD: 0,
                audioDurationSeconds: audioDuration,
                latencyMs: latencyMs
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

    /// The synchronous endpoint returns the transcript inline, so this is just
    /// a wrapper that unwraps the cached result. The shape is preserved for
    /// `TranscriptionQueue` (and any future async/webhook path).
    func pollResult(_ job: ScribeJob) async throws -> Transcript {
        guard let raw = job.inlineResult else { throw ScribeError.invalidResponse }
        return Transcript.fromScribeRaw(raw, episodeID: job.episodeID, languageHint: job.languageHint)
    }

    // MARK: Multipart

    /// One of the two valid audio sources for the Scribe request. Distinct
    /// because the multipart encoding differs: a remote URL goes into a
    /// `source_url` text field; a local file goes into a `file` binary field.
    enum AudioField: Sendable {
        case file(url: URL, filename: String, contentType: String)
        case sourceURL(String)

        var kind: String {
            switch self {
            case .file: return "file"
            case .sourceURL: return "source_url"
            }
        }
    }

    /// Picks the right multipart audio source for `audioURL`. `file://` URLs
    /// are encoded as binary `file` fields; `https://` URLs are passed as
    /// `source_url` so ElevenLabs fetches them server-side (avoids a 100MB
    /// in-memory client-side download for every transcription).
    static func audioField(for audioURL: URL) throws -> AudioField {
        if audioURL.isFileURL {
            // Confirm the file actually exists before we try to encode it.
            guard FileManager.default.fileExists(atPath: audioURL.path) else {
                throw ScribeError.invalidAudioURL
            }
            return .file(
                url: audioURL,
                filename: audioURL.lastPathComponent,
                contentType: contentType(for: audioURL.pathExtension)
            )
        }
        guard let scheme = audioURL.scheme?.lowercased(),
              scheme == "https" || scheme == "http" else {
            throw ScribeError.invalidAudioURL
        }
        return .sourceURL(audioURL.absoluteString)
    }

    /// Best-effort MIME inference from a path extension. ElevenLabs accepts
    /// the common podcast formats; the wrong MIME doesn't reject the upload
    /// (the server sniffs), but a sane value helps logging and proxies.
    static func contentType(for pathExtension: String) -> String {
        switch pathExtension.lowercased() {
        case "mp3":  return "audio/mpeg"
        case "m4a", "m4b", "aac": return "audio/mp4"
        case "wav":  return "audio/wav"
        case "ogg":  return "audio/ogg"
        case "opus": return "audio/opus"
        case "flac": return "audio/flac"
        case "webm": return "audio/webm"
        default:     return "application/octet-stream"
        }
    }

    static func multipartBody(
        boundary: String,
        modelID: String,
        languageHint: String?,
        audio: AudioField
    ) throws -> Data {
        var body = Data()
        let crlf = "\r\n"

        func appendField(_ name: String, _ value: String) {
            body.append("--\(boundary)\(crlf)".data(using: .utf8)!)
            body.append("Content-Disposition: form-data; name=\"\(name)\"\(crlf)\(crlf)".data(using: .utf8)!)
            body.append("\(value)\(crlf)".data(using: .utf8)!)
        }

        appendField("model_id", modelID)
        appendField("diarize", "true")
        appendField("timestamps_granularity", "word")
        appendField("tag_audio_events", "true")
        if let hint = languageHint, !hint.isEmpty {
            appendField("language_code", hint)
        }

        switch audio {
        case .sourceURL(let urlString):
            // ElevenLabs fetches this URL server-side. No client-side download.
            appendField("source_url", urlString)
        case .file(let url, let filename, let contentType):
            body.append("--\(boundary)\(crlf)".data(using: .utf8)!)
            body.append("Content-Disposition: form-data; name=\"file\"; filename=\"\(filename)\"\(crlf)".data(using: .utf8)!)
            body.append("Content-Type: \(contentType)\(crlf)\(crlf)".data(using: .utf8)!)
            body.append(try Data(contentsOf: url, options: .mappedIfSafe))
            body.append(crlf.data(using: .utf8)!)
        }
        body.append("--\(boundary)--\(crlf)".data(using: .utf8)!)
        return body
    }

    // MARK: HTTP

    static func assertOK(response: URLResponse, data: Data) throws {
        guard let http = response as? HTTPURLResponse else { throw ScribeError.invalidResponse }
        guard (200..<300).contains(http.statusCode) else {
            throw ScribeError.http(status: http.statusCode, body: String(data: data, encoding: .utf8))
        }
    }
}

// MARK: - DTOs

struct ScribeJob: Sendable, Hashable {
    let requestID: String
    let episodeID: UUID
    let createdAt: Date
    let languageHint: String?
    let inlineResult: ScribeRawResult?
}

struct ScribeRawResult: Codable, Sendable, Hashable {
    let language_code: String?
    let text: String?
    let words: [ScribeWord]?
}

struct ScribeWord: Codable, Sendable, Hashable {
    let text: String
    let start: Double
    let end: Double
    let type: String?           // "word" | "spacing" | "audio_event"
    let speaker_id: String?
}

// MARK: - Transcript adapter

extension Transcript {
    /// Converts a Scribe raw result into our internal `Transcript`. Words of
    /// type `spacing` are dropped. Words of type `audio_event` (`[laughter]`,
    /// `[music]`) are folded into the body text in-place — the wiki / agent
    /// surfaces will use them for context, the reader will hide them.
    static func fromScribeRaw(
        _ raw: ScribeRawResult,
        episodeID: UUID,
        languageHint: String?
    ) -> Transcript {
        let language = raw.language_code ?? languageHint ?? "en-US"
        let words = raw.words ?? []

        // Group words into segments by speaker switch and ≥1.2s pause boundary.
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
            } else { speakerID = nil }
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
