import Foundation
import os.log

// MARK: - AssemblyAITranscriptClient
//
// Pre-recorded transcription via AssemblyAI (`api.assemblyai.com`). URL-based
// input: we pass the publisher's enclosure URL and AssemblyAI fetches it
// server-side - no upload, no base64 inflation, no on-device memory pressure
// for 90+ MB podcast files. Returns per-utterance and per-word timestamps
// which the Rust chapter qualifier uses to anchor boundaries and ad spans.
//
// Wire-protocol notes per AssemblyAI's published llms-full.txt:
//   - Auth header: `Authorization: <raw_key>` - NO `Bearer` prefix. This is
//     a deliberate AssemblyAI convention (Voice Agent API is the only one of
//     their products that uses Bearer).
//   - Submit: `POST /v2/transcript` with `application/json`. Required:
//     `audio_url`, `speech_models`. The `speech_models` array is an ORDERED
//     FALLBACK LIST - first model is tried, on availability failure the
//     gateway falls through to the next. A single transcript is produced by
//     exactly one model. Default we send: `["universal-3-pro","universal-2"]`.
//   - Status: `GET /v2/transcript/{id}`. One native read reports `queued`,
//     `processing`, `completed`, or `error`; Rust schedules any later read.
//   - Timestamps are in MILLISECONDS in the response (`start`, `end`). We
//     divide by 1000 to land on our seconds-based `Transcript.Segment`.
//   - `audio_duration` is in seconds (the one exception).
//   - Max file size 5 GB / 10 hr per file via `audio_url`. Plenty for podcasts.
//   - Local files: we'd need `POST /v2/upload` with raw binary body (NOT
//     multipart), max 2.2 GB, returns `{ upload_url }`. Not wired in this v1
//     because every episode that lacks a local file always has an enclosure
//     URL - and we prefer the URL anyway to avoid client-side upload.
//
// Submission and status observation remain separate so a reconstructed Rust
// workflow can recover by durable provider ID without resubmitting paid work.

actor AssemblyAITranscriptClient {

    enum TranscribeError: Swift.Error, LocalizedError, Sendable {
        case missingAPIKey
        case invalidAudioURL
        case invalidResponse
        case http(status: Int)
        case decoding
        case remoteError
        case cancelled
        case timedOut

        /// User-facing copy. Lands in the audit log "User-facing message" detail
        /// and on the `TranscribingInProgressView` failure panel.
        var errorDescription: String? {
            switch self {
            case .missingAPIKey:
                return "Add an AssemblyAI API key in Settings > Intelligence > Providers to transcribe episodes."
            case .invalidAudioURL:
                return "Couldn't find the episode audio to transcribe."
            case .invalidResponse:
                return "AssemblyAI returned an unexpected response. Try again in a moment."
            case .http(let status) where status == 401 || status == 403:
                return "AssemblyAI rejected your API key. Update it in Settings > Intelligence > Providers."
            case .http(let status) where status == 422:
                return "AssemblyAI couldn't process the audio (file format or URL not accepted)."
            case .http(let status) where status == 429:
                return "AssemblyAI rate-limited the request. Wait a minute and retry."
            case .http(let status) where status >= 500:
                return "AssemblyAI is having trouble (\(status)). Retry in a few minutes."
            case .http(let status):
                return "AssemblyAI returned an unexpected error (\(status))."
            case .decoding:
                return "AssemblyAI returned a transcript shape we couldn't read."
            case .remoteError:
                return "AssemblyAI couldn't transcribe this episode."
            case .cancelled:
                return "Transcription cancelled."
            case .timedOut:
                return "Transcription took too long. Try again - long episodes can take several minutes."
            }
        }

    }

    private static let logger = Logger.app("AssemblyAITranscriptClient")

    /// Decoder shared across calls - `JSONDecoder` is Sendable and reentrant
    /// after construction, so a single instance is fine.
    static let decoder = JSONDecoder()

    /// Submit request timeout - short, the submit endpoint just validates and
    /// returns immediately with a queued job ID.
    static let submitTimeout: TimeInterval = 30

    let baseURL: URL
    let session: URLSession
    let credential: @Sendable () throws -> String?

    init(
        baseURL: URL = URL(string: "https://api.assemblyai.com")!,
        session: URLSession = .shared,
        credential: @escaping @Sendable () throws -> String? = { try AssemblyAICredentialStore.apiKey() }
    ) {
        self.baseURL = baseURL
        self.session = session
        self.credential = credential
    }

    // MARK: - API

    /// Submits an audio URL for transcription and returns a job handle. The
    /// Rust drives later status observations through typed recovery requests.
    ///
    /// `speechModels` is the ordered fallback list (e.g. `["universal-3-pro",
    /// "universal-2"]`). `audioURL` must be HTTPS - AssemblyAI fetches it
    /// server-side. Local-file uploads aren't supported by this v1.
    func submit(
        audioURL: URL,
        episodeID: UUID,
        speechModels: [String],
        speakerLabels: Bool,
        languageDetection: Bool,
        languageHint: String? = nil
    ) async throws -> AssemblyAIJob {
        try Task.checkCancellation()
        guard let key = try credential(), !key.isEmpty else { throw TranscribeError.missingAPIKey }

        // Only remote HTTPS URLs are supported in v1. Local-file uploads via
        // /v2/upload would need raw-binary streaming; punt until a user needs it.
        guard let scheme = audioURL.scheme?.lowercased(), scheme == "https" || scheme == "http" else {
            throw TranscribeError.invalidAudioURL
        }

        var body: [String: Any] = [
            "audio_url": audioURL.absoluteString,
            "speech_models": speechModels,
            "speaker_labels": speakerLabels,
        ]
        if languageDetection {
            body["language_detection"] = true
        } else if let hint = languageHint, !hint.isEmpty {
            // `language_code` is pre-recorded-only. Format is ISO like "en_us";
            // we forward the caller-provided hint verbatim.
            body["language_code"] = hint
        }

        let endpoint = baseURL
            .appendingPathComponent("v2")
            .appendingPathComponent("transcript")
        var request = URLRequest(url: endpoint)
        request.httpMethod = "POST"
        request.setValue(key, forHTTPHeaderField: "Authorization")
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        request.timeoutInterval = Self.submitTimeout
        request.httpBody = try JSONSerialization.data(withJSONObject: body)

        Self.logger.info(
            "submit - models=\(speechModels.joined(separator: ","), privacy: .public) diarize=\(speakerLabels, privacy: .public) detectLang=\(languageDetection, privacy: .public) url=\(audioURL.host ?? "", privacy: .public)"
        )

        let data: Data
        let response: URLResponse
        do {
            (data, response) = try await session.data(for: request)
        } catch is CancellationError {
            throw TranscribeError.cancelled
        } catch let error as URLError where error.code == .cancelled {
            throw TranscribeError.cancelled
        } catch let error as URLError where error.code == .timedOut {
            throw TranscribeError.timedOut
        }

        try Self.assertOK(response: response, data: data)

        let payload: AssemblyAITranscriptPayload
        do {
            payload = try Self.decoder.decode(AssemblyAITranscriptPayload.self, from: data)
        } catch {
            Self.logger.error("AssemblyAI submit response could not be decoded")
            throw TranscribeError.decoding
        }

        guard let id = payload.id else {
            throw TranscribeError.invalidResponse
        }

        return AssemblyAIJob(
            transcriptID: id,
            episodeID: episodeID,
            createdAt: Date(),
            languageHint: languageHint,
            speechModels: speechModels
        )
    }

    // MARK: - HTTP

    static func assertOK(response: URLResponse, data: Data) throws {
        guard let http = response as? HTTPURLResponse else { throw TranscribeError.invalidResponse }
        guard (200..<300).contains(http.statusCode) else {
            throw TranscribeError.http(status: http.statusCode)
        }
    }
}

// MARK: - DTOs

struct AssemblyAIJob: Sendable, Hashable {
    let transcriptID: String
    let episodeID: UUID
    let createdAt: Date
    let languageHint: String?
    /// Verbatim `speech_models` list submitted. Surfaced on the cost-ledger
    /// record so the Usage view shows which model the user picked even though
    /// AssemblyAI's response doesn't echo it.
    let speechModels: [String]
}

/// Single response shape that covers both the submit reply and the poll reply
/// - they're the same envelope, differing only in which fields are populated.
struct AssemblyAITranscriptPayload: Codable, Sendable, Hashable {
    let id: String?
    let status: String?
    let audio_url: String?
    let audio_duration: Double?     // seconds (the one exception to ms timestamps)
    let language_code: String?
    let text: String?
    let error: String?
    let words: [AssemblyAIWord]?
    let utterances: [AssemblyAIUtterance]?
    /// Billing / usage telemetry populated on the completed poll. Per
    /// AssemblyAI's OpenAPI: `cost` is USD, `seconds` is the audio duration
    /// processed, `input_tokens` / `output_tokens` are model-side counts.
    let usage: AssemblyAIUsage?
}

struct AssemblyAIUsage: Codable, Sendable, Hashable {
    let cost: Double?
    let seconds: Double?
    let input_tokens: Int?
    let output_tokens: Int?
    let total_tokens: Int?
}

/// Utterance = diarized turn. Mapped 1:1 onto our `Transcript.Segment` when
/// `speaker_labels=true`. Timestamps are MILLISECONDS - convert to seconds at
/// the adapter boundary.
struct AssemblyAIUtterance: Codable, Sendable, Hashable {
    let start: Int
    let end: Int
    let text: String
    let confidence: Double?
    let speaker: String?
    let words: [AssemblyAIWord]?
}

/// Word with character-level timestamps. AssemblyAI always returns these on
/// pre-recorded transcripts. Timestamps in MILLISECONDS.
struct AssemblyAIWord: Codable, Sendable, Hashable {
    let start: Int
    let end: Int
    let text: String
    let confidence: Double?
    let speaker: String?
}

// MARK: - Transcript adapter

extension Transcript {
    /// Converts an AssemblyAI completed payload into our internal `Transcript`.
    ///
    /// Segment-building strategy:
    ///   - If `utterances` is populated (speaker_labels was on) -> use one
    ///     `Segment` per utterance. Speaker IDs from "A", "B"... map onto our
    ///     `Speaker` records.
    ///   - Otherwise -> group raw `words` into approximate sentence segments
    ///     using a 1.5 s pause boundary heuristic. Mirrors the Scribe adapter's
    ///     behaviour when diarization is off.
    ///
    /// Timestamps in milliseconds are converted to seconds at this boundary so
    /// the rest of the app keeps its `TimeInterval`-in-seconds invariant.
    static func fromAssemblyAI(
        _ payload: AssemblyAITranscriptPayload,
        episodeID: UUID,
        languageHint: String?
    ) -> Transcript {
        let language = payload.language_code ?? languageHint ?? "en-US"

        // Build the speakers map first so segment construction can reference it.
        var speakers: [String: Speaker] = [:]
        if let utterances = payload.utterances {
            for u in utterances {
                guard let label = u.speaker, !label.isEmpty, speakers[label] == nil else { continue }
                speakers[label] = Speaker(label: label, displayName: "Speaker \(label)")
            }
        }

        let segments: [Segment]
        if let utterances = payload.utterances, !utterances.isEmpty {
            segments = utterances.map { u in
                let words = (u.words ?? []).map { w in
                    Word(
                        start: Double(w.start) / 1000.0,
                        end: Double(w.end) / 1000.0,
                        text: w.text
                    )
                }
                return Segment(
                    start: Double(u.start) / 1000.0,
                    end: Double(u.end) / 1000.0,
                    speakerID: u.speaker.flatMap { speakers[$0]?.id },
                    text: u.text.trimmingCharacters(in: .whitespacesAndNewlines),
                    words: words.isEmpty ? nil : words
                )
            }
        } else {
            // No diarization - group words into pseudo-segments at pause boundaries.
            segments = groupWordsIntoSegments(payload.words ?? [])
        }

        return Transcript(
            episodeID: episodeID,
            language: language,
            source: .assemblyAI,
            segments: segments,
            speakers: Array(speakers.values),
            generatedAt: Date()
        )
    }

    /// Falls-back segment construction when `utterances` is absent. 1.5 s pause
    /// boundary keeps segments human-sized for the player's transcript view
    /// without losing shared-core anchoring (it needs timestamped lines, not
    /// perfect sentence breaks).
    private static func groupWordsIntoSegments(_ words: [AssemblyAIWord]) -> [Segment] {
        guard !words.isEmpty else { return [] }
        let pauseBoundary = 1.5
        var segments: [Segment] = []
        var bufferStart = Double(words[0].start) / 1000.0
        var bufferEnd = Double(words[0].end) / 1000.0
        var bufferText = words[0].text
        var bufferWords: [Word] = [
            Word(start: Double(words[0].start) / 1000.0,
                 end: Double(words[0].end) / 1000.0,
                 text: words[0].text)
        ]

        func flush() {
            let trimmed = bufferText.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !trimmed.isEmpty else { return }
            segments.append(Segment(
                start: bufferStart,
                end: bufferEnd,
                speakerID: nil,
                text: trimmed,
                words: bufferWords.isEmpty ? nil : bufferWords
            ))
        }

        for word in words.dropFirst() {
            let start = Double(word.start) / 1000.0
            let end = Double(word.end) / 1000.0
            if start - bufferEnd >= pauseBoundary {
                flush()
                bufferStart = start
                bufferEnd = end
                bufferText = word.text
                bufferWords = [Word(start: start, end: end, text: word.text)]
            } else {
                bufferText += " " + word.text
                bufferEnd = end
                bufferWords.append(Word(start: start, end: end, text: word.text))
            }
        }
        flush()
        return segments
    }
}
