import Foundation
import os.log

// MARK: - TTSClientProtocol

/// Streaming text-to-speech client.
///
/// `synthesizeStream(text:voice:)` returns an `AsyncThrowingStream` of raw
/// audio frame `Data`. The caller (typically `AudioConversationManager`)
/// pipes those frames into an `AVAudioEngine` player node.
///
/// Implementations are responsible for chunking strategy, network, auth, and
/// error reporting. They do NOT touch `AVAudioSession` — that's the audio
/// coordinator's job.
protocol TTSClientProtocol: Sendable {
    func synthesizeStream(text: String, voiceID: String) -> AsyncThrowingStream<Data, Error>

    /// Whether this client is configured (credentials present, network
    /// reachable, etc). Used by the manager to fall back to AVSpeech when
    /// false.
    var isConfigured: Bool { get }
}

// MARK: - ElevenLabsTTSError

enum ElevenLabsTTSError: Error, Equatable, Sendable {
    case missingAPIKey
    case webSocketFailed(String)
    case restFailed(Int)
    case decodeFailed(String)
}

// MARK: - ElevenLabsTTSClient

/// ElevenLabs Flash v2.5 streaming client.
///
/// **First-byte target**: ElevenLabs publish a sub-100ms first-byte latency
/// for Flash v2.5 over their WebSocket "stream-input" endpoint. We open the
/// socket, send the API key + initial config, then push the text chunks.
/// Audio frames stream back as base64 PCM/MP3 chunks which we decode and
/// forward.
///
/// **Falls back** to the REST `/text-to-speech/{voice}/stream` endpoint when
/// the socket fails to upgrade — that endpoint backs ElevenLabs' Multilingual
/// v2 model which the spec calls out as "briefing-grade" quality. We pay a
/// few hundred ms in first-byte but the audio is uninterrupted.
///
/// **Auth**: API key is read fresh from `ElevenLabsCredentialStore` on each
/// call so user rotations take effect without restart.
///
/// **Protocol assumption flagged in the work report**: the stream-input
/// endpoint URL and JSON envelope below match the public docs but should
/// be verified against ElevenLabs' canonical schema at integration time —
/// the codebase has no other usage of this WebSocket today.
final class ElevenLabsTTSClient: TTSClientProtocol, @unchecked Sendable {

    private let logger = Logger.app("ElevenLabsTTSClient")
    private let urlSession: URLSession

    /// Default voice ID used when none is supplied by the caller.
    /// "Rachel" — ElevenLabs' default professional voice.
    static let defaultVoiceID = "21m00Tcm4TlvDq8ikWAM"

    /// Default model ID for the Flash WebSocket. Flash v2.5 is the target
    /// per `voice-stt-tts-stack.md`.
    static let flashModelID = "eleven_flash_v2_5"

    /// REST fallback model. Multilingual v2 = briefing-grade quality.
    static let restModelID = "eleven_multilingual_v2"

    init(urlSession: URLSession = .shared) {
        self.urlSession = urlSession
    }

    var isConfigured: Bool {
        ElevenLabsCredentialStore.hasAPIKey()
    }

    func synthesizeStream(text: String, voiceID: String) -> AsyncThrowingStream<Data, Error> {
        AsyncThrowingStream { continuation in
            let task = Task {
                do {
                    let key = try Self.readAPIKey()
                    try await self.streamViaWebSocket(
                        text: text,
                        voiceID: voiceID,
                        apiKey: key,
                        continuation: continuation
                    )
                } catch let wsError {
                    // WebSocket attempt failed — try REST as a graceful fallback.
                    self.logger.notice("WS TTS failed, falling back to REST: \(wsError.localizedDescription, privacy: .public)")
                    do {
                        let key = try Self.readAPIKey()
                        try await self.streamViaREST(
                            text: text,
                            voiceID: voiceID,
                            apiKey: key,
                            continuation: continuation
                        )
                        continuation.finish()
                    } catch {
                        continuation.finish(throwing: error)
                    }
                }
            }
            continuation.onTermination = { @Sendable _ in
                task.cancel()
            }
        }
    }

    // MARK: - Private — WebSocket path

    private func streamViaWebSocket(
        text: String,
        voiceID: String,
        apiKey: String,
        continuation: AsyncThrowingStream<Data, Error>.Continuation
    ) async throws {
        guard let url = URL(string: "wss://api.elevenlabs.io/v1/text-to-speech/\(voiceID)/stream-input?model_id=\(Self.flashModelID)") else {
            throw ElevenLabsTTSError.webSocketFailed("Invalid URL")
        }

        var request = URLRequest(url: url)
        request.setValue(apiKey, forHTTPHeaderField: "xi-api-key")
        let socket = urlSession.webSocketTask(with: request)
        socket.resume()

        // 1. Initial config message — opens the session and sets voice settings.
        let initPayload: [String: Any] = [
            "text": " ",
            "voice_settings": [
                "stability": 0.5,
                "similarity_boost": 0.8,
            ],
            "xi_api_key": apiKey,
            "generation_config": [
                "chunk_length_schedule": [50, 90, 120, 200],
            ],
        ]
        try await sendJSON(initPayload, on: socket)

        // 2. Send the text in one chunk. For very long text we'd split into
        //    smaller pushes to maximise responsiveness — left as future work.
        let textPayload: [String: Any] = [
            "text": text,
            "try_trigger_generation": true,
        ]
        try await sendJSON(textPayload, on: socket)

        // 3. End-of-input sentinel.
        let endPayload: [String: Any] = ["text": ""]
        try await sendJSON(endPayload, on: socket)

        // 4. Drain audio frames until the socket closes.
        try await drain(socket: socket, continuation: continuation)
        socket.cancel(with: .goingAway, reason: nil)
        continuation.finish()
    }

    private func sendJSON(_ object: [String: Any], on socket: URLSessionWebSocketTask) async throws {
        let data = try JSONSerialization.data(withJSONObject: object, options: [])
        guard let string = String(data: data, encoding: .utf8) else {
            throw ElevenLabsTTSError.webSocketFailed("Could not encode JSON")
        }
        try await socket.send(.string(string))
    }

    private func drain(
        socket: URLSessionWebSocketTask,
        continuation: AsyncThrowingStream<Data, Error>.Continuation
    ) async throws {
        while !Task.isCancelled {
            let message = try await socket.receive()
            switch message {
            case .string(let str):
                if let frameData = str.data(using: .utf8),
                   let json = try? JSONSerialization.jsonObject(with: frameData) as? [String: Any] {
                    if let audioBase64 = json["audio"] as? String,
                       let audio = Data(base64Encoded: audioBase64) {
                        continuation.yield(audio)
                    }
                    if let isFinal = json["isFinal"] as? Bool, isFinal {
                        return
                    }
                    if let errorMsg = json["error"] as? String {
                        throw ElevenLabsTTSError.webSocketFailed(errorMsg)
                    }
                }
            case .data(let data):
                continuation.yield(data)
            @unknown default:
                break
            }
        }
    }

    // MARK: - Private — REST fallback path

    private func streamViaREST(
        text: String,
        voiceID: String,
        apiKey: String,
        continuation: AsyncThrowingStream<Data, Error>.Continuation
    ) async throws {
        guard let url = URL(string: "https://api.elevenlabs.io/v1/text-to-speech/\(voiceID)/stream?optimize_streaming_latency=2") else {
            throw ElevenLabsTTSError.restFailed(-1)
        }
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue(apiKey, forHTTPHeaderField: "xi-api-key")
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue("audio/mpeg", forHTTPHeaderField: "Accept")
        let body: [String: Any] = [
            "text": text,
            "model_id": Self.restModelID,
            "voice_settings": [
                "stability": 0.5,
                "similarity_boost": 0.8,
            ],
        ]
        request.httpBody = try JSONSerialization.data(withJSONObject: body)

        let (bytes, response) = try await urlSession.bytes(for: request)
        guard let http = response as? HTTPURLResponse, (200...299).contains(http.statusCode) else {
            let code = (response as? HTTPURLResponse)?.statusCode ?? -1
            throw ElevenLabsTTSError.restFailed(code)
        }

        // Flush in modest chunks (~4KB) so playback can begin while the
        // download is still arriving. AsyncBytes reads byte-by-byte so we
        // batch up before yielding to keep allocator pressure reasonable.
        var buffer = Data()
        buffer.reserveCapacity(4096)
        for try await byte in bytes {
            if Task.isCancelled { return }
            buffer.append(byte)
            if buffer.count >= 4096 {
                continuation.yield(buffer)
                buffer.removeAll(keepingCapacity: true)
            }
        }
        if !buffer.isEmpty {
            continuation.yield(buffer)
        }
    }

    private static func readAPIKey() throws -> String {
        guard let key = try ElevenLabsCredentialStore.apiKey() else {
            throw ElevenLabsTTSError.missingAPIKey
        }
        return key
    }
}
