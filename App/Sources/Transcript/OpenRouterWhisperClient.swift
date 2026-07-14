import Foundation
import os.log

// MARK: - OpenRouterWhisperClient

/// REST client for Whisper transcription via OpenRouter's OpenAI-compatible
/// audio transcription endpoint (`POST /api/v1/audio/transcriptions`).
///
/// Unlike ElevenLabs Scribe, OpenRouter's endpoint only accepts file uploads —
/// there is no `source_url` option. For local files (downloaded episodes) the
/// bytes are uploaded directly. For remote HTTPS URLs, the audio is first
/// downloaded to a temp file, uploaded, then cleaned up.
actor OpenRouterWhisperClient {

    enum WhisperError: Swift.Error, LocalizedError, Sendable {
        case missingAPIKey
        case invalidAudioURL
        case downloadFailed(String)
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
    nonisolated(unsafe) private static let decoder = JSONDecoder()

    /// 10 minutes — large audio files can take several minutes server-side.
    static let requestTimeout: TimeInterval = 600

    private let baseURL: URL
    private let session: URLSession
    private let model: String
    private let credential: @Sendable () throws -> String?

    init(
        baseURL: URL = URL(string: "https://openrouter.ai")!,
        model: String = "openai/whisper-1",
        session: URLSession = .shared,
        credential: @escaping @Sendable () throws -> String? = { try OpenRouterCredentialStore.apiKey() }
    ) {
        self.baseURL = baseURL
        self.model = model
        self.session = session
        self.credential = credential
    }

    // MARK: - API

    func transcribe(audioURL: URL, episodeID: UUID, languageHint: String? = nil) async throws -> Transcript {
        try Task.checkCancellation()
        guard let key = try credential(), !key.isEmpty else { throw WhisperError.missingAPIKey }

        let fileURL = try await resolveLocalFile(from: audioURL)
        let isTemp = !audioURL.isFileURL
        defer {
            if isTemp { try? FileManager.default.removeItem(at: fileURL) }
        }

        let endpoint = baseURL.appendingPathComponent("api/v1/audio/transcriptions")
        let boundary = "Boundary-\(UUID().uuidString)"
        var request = URLRequest(url: endpoint)
        request.httpMethod = "POST"
        request.setValue("Bearer \(key)", forHTTPHeaderField: "Authorization")
        request.setValue("multipart/form-data; boundary=\(boundary)", forHTTPHeaderField: "Content-Type")
        request.timeoutInterval = Self.requestTimeout

        let body = try buildBody(boundary: boundary, fileURL: fileURL, languageHint: languageHint)

        try Task.checkCancellation()
        Self.logger.info(
            "submitting Whisper request — model=\(self.model, privacy: .public) bytes=\(body.count, privacy: .public)"
        )

        let submitStart = Date()
        let data: Data
        let response: URLResponse
        do {
            (data, response) = try await session.upload(for: request, from: body)
        } catch is CancellationError {
            throw WhisperError.cancelled
        } catch let error as URLError where error.code == .cancelled {
            throw WhisperError.cancelled
        } catch let error as URLError where error.code == .timedOut {
            throw WhisperError.timedOut
        }

        try Task.checkCancellation()
        guard let http = response as? HTTPURLResponse else { throw WhisperError.invalidResponse }
        guard (200..<300).contains(http.statusCode) else {
            let body = String(data: data, encoding: .utf8)
            Self.logger.error("Whisper HTTP \(http.statusCode, privacy: .public): \(body ?? "", privacy: .public)")
            throw WhisperError.http(status: http.statusCode, body: body)
        }

        let raw: WhisperVerboseResponse
        do {
            raw = try Self.decoder.decode(WhisperVerboseResponse.self, from: data)
        } catch {
            let preview = String(data: data.prefix(500), encoding: .utf8) ?? "<binary>"
            Self.logger.error("Whisper decode failed: \(String(describing: error), privacy: .public) body=\(preview, privacy: .public)")
            throw WhisperError.decoding("Could not decode transcription response: \(error)")
        }

        // OpenRouter Whisper response has no `cost` field; the gateway strips
        // upstream billing detail. Log activity + duration so the Usage view
        // shows the call; user's OpenRouter dashboard is the source of truth
        // for $$ cost.
        let latencyMs = Int(Date().timeIntervalSince(submitStart) * 1000)
        let modelLabel = self.model
        let durationSeconds = raw.duration
        Task { @MainActor in
            CostLedger.shared.logSTT(
                feature: CostFeature.sttOpenRouterWhisper,
                model: modelLabel,
                costUSD: 0,
                audioDurationSeconds: durationSeconds,
                latencyMs: latencyMs
            )
        }

        return Transcript.fromWhisperResponse(raw, episodeID: episodeID)
    }

    // MARK: - Private

    private func resolveLocalFile(from audioURL: URL) async throws -> URL {
        if audioURL.isFileURL {
            guard FileManager.default.fileExists(atPath: audioURL.path) else {
                throw WhisperError.invalidAudioURL
            }
            return audioURL
        }
        // OpenRouter Whisper only accepts file uploads — download to temp first.
        Self.logger.info("downloading remote audio for Whisper upload: \(audioURL.host ?? "", privacy: .public)")
        let tempURL: URL
        do {
            let (downloaded, _) = try await session.download(from: audioURL)
            tempURL = downloaded
        } catch is CancellationError {
            throw WhisperError.cancelled
        } catch {
            throw WhisperError.downloadFailed(error.localizedDescription)
        }
        let ext = audioURL.pathExtension.isEmpty ? "mp3" : audioURL.pathExtension
        let stableURL = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString)
            .appendingPathExtension(ext)
        do {
            try FileManager.default.moveItem(at: tempURL, to: stableURL)
        } catch {
            throw WhisperError.downloadFailed("Could not stage temp file: \(error.localizedDescription)")
        }
        return stableURL
    }

    private func buildBody(boundary: String, fileURL: URL, languageHint: String?) throws -> Data {
        let crlf = "\r\n"
        var body = Data()

        func field(_ name: String, _ value: String) {
            body.append("--\(boundary)\(crlf)".data(using: .utf8)!)
            body.append("Content-Disposition: form-data; name=\"\(name)\"\(crlf)\(crlf)".data(using: .utf8)!)
            body.append("\(value)\(crlf)".data(using: .utf8)!)
        }

        field("model", model)
        field("response_format", "verbose_json")
        field("timestamp_granularities[]", "segment")
        if let hint = languageHint, !hint.isEmpty {
            field("language", hint)
        }

        let filename = fileURL.lastPathComponent
        let ct = ElevenLabsScribeClient.contentType(for: fileURL.pathExtension)
        body.append("--\(boundary)\(crlf)".data(using: .utf8)!)
        body.append("Content-Disposition: form-data; name=\"file\"; filename=\"\(filename)\"\(crlf)".data(using: .utf8)!)
        body.append("Content-Type: \(ct)\(crlf)\(crlf)".data(using: .utf8)!)
        body.append(try Data(contentsOf: fileURL, options: .mappedIfSafe))
        body.append(crlf.data(using: .utf8)!)
        body.append("--\(boundary)--\(crlf)".data(using: .utf8)!)
        return body
    }
}

// MARK: - DTOs

struct WhisperVerboseResponse: Codable, Sendable {
    let task: String?
    let language: String?
    let duration: Double?
    let text: String?
    let segments: [WhisperSegment]?
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
