import AVFoundation
import Foundation

/// Realtime speech-to-text powered by ElevenLabs WebSocket API.
///
/// Ported from the win-the-day app's `ElevenLabsRealtimeSTT`. Drops the
/// local `VoiceRecordingDraftStore` integration — this surface only needs
/// the live in-memory transcript. Audio session management uses direct
/// `AVAudioSession` configuration rather than `AudioSessionCoordinator`
/// so playback pause/resume stays entirely in the sheet.
@MainActor
final class VoiceNoteRealtimeSTT: ObservableObject {

    @Published private(set) var isRecording = false
    @Published private(set) var isStarting = false
    @Published private(set) var level: Float = 0
    @Published private(set) var transcript: String = ""
    @Published private(set) var errorMessage: String?
    @Published private(set) var statusMessage = "Idle"

    private let sampleRate = 16_000
    private let vadSilenceThresholdSecs = 1.2
    private let vadThreshold = 0.4

    private var webSocketTask: URLSessionWebSocketTask?
    private var connectionTask: Task<Void, Never>?
    private var receiveTask: Task<Void, Never>?
    private var audioCapture: VNAudioCapture?
    private var committedSegments: [String] = []
    private var partialTranscript = ""
    private var pendingAudioChunks: [Data] = []
    private var isSendingAudio = false
    private var shouldAcceptAudio = false
    private var shouldQueueAudio = false
    private var shouldSendAudio = false
    private var isClosing = false
    private var didActivateSession = false

    func start(modelID configuredModelID: String) async throws {
        guard !isRecording, !isStarting else { return }

        isStarting = true
        statusMessage = "Preparing microphone"
        defer {
            isStarting = false
            if !isRecording, errorMessage == nil { statusMessage = "Idle" }
        }
        resetTranscript()
        errorMessage = nil
        isClosing = false
        pendingAudioChunks.removeAll()
        connectionTask?.cancel()

        let granted = await AVAudioApplication.requestRecordPermission()
        guard granted else { throw VoiceNoteSTTError.micPermissionDenied }

        let apiKey = try? ElevenLabsCredentialStore.apiKey()
        shouldAcceptAudio = true
        shouldQueueAudio = apiKey != nil
        shouldSendAudio = false

        do {
            try startAudioCapture()
            isRecording = true
            statusMessage = apiKey == nil ? "No ElevenLabs key" : "Connecting"
        } catch {
            shouldAcceptAudio = false
            shouldQueueAudio = false
            shouldSendAudio = false
            closeSocket()
            throw error
        }

        guard let key = apiKey else { return }

        connectionTask = Task { @MainActor [weak self] in
            await self?.connectRealtime(apiKey: key, configuredModelID: configuredModelID)
        }
    }

    func stop() async -> String {
        guard isRecording || webSocketTask != nil else {
            return transcript.trimmingCharacters(in: .whitespacesAndNewlines)
        }

        shouldAcceptAudio = false
        shouldQueueAudio = false
        connectionTask?.cancel()
        connectionTask = nil
        stopAudioCapture()
        isRecording = false
        statusMessage = "Finishing transcript"
        level = 0

        let deadline = Date().addingTimeInterval(1.0)
        while (isSendingAudio || !pendingAudioChunks.isEmpty) && Date() < deadline {
            try? await Task.sleep(nanoseconds: 50_000_000)
        }
        try? await Task.sleep(nanoseconds: 350_000_000)
        closeSocket()
        statusMessage = "Idle"
        return transcript.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    func cancel() {
        shouldAcceptAudio = false
        shouldQueueAudio = false
        connectionTask?.cancel()
        connectionTask = nil
        stopAudioCapture()
        closeSocket()
        resetTranscript()
        isRecording = false
        isStarting = false
        statusMessage = "Idle"
        level = 0
    }

    // MARK: - Connection

    private func connectRealtime(apiKey: String, configuredModelID: String) async {
        do {
            let token = try await createRealtimeToken(apiKey: apiKey)
            try Task.checkCancellation()
            guard isRecording, shouldAcceptAudio else { return }

            let request = try makeWebSocketRequest(
                token: token,
                modelID: realtimeModelID(from: configuredModelID)
            )
            let task = URLSession.shared.webSocketTask(with: request)
            webSocketTask = task
            isClosing = false
            shouldSendAudio = true
            task.resume()

            receiveTask = Task { @MainActor [weak self] in
                await self?.receiveLoop()
            }
            statusMessage = "Listening"
            startDrainingAudioQueue()
        } catch is CancellationError {
            return
        } catch {
            guard isRecording || shouldAcceptAudio else { return }
            realtimeUnavailable(with: error.localizedDescription)
        }
    }

    private func createRealtimeToken(apiKey: String) async throws -> String {
        guard let url = URL(string: "https://api.elevenlabs.io/v1/single-use-token/realtime_scribe") else {
            throw VoiceNoteSTTError.invalidResponse
        }
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue(apiKey, forHTTPHeaderField: "xi-api-key")
        let (data, _) = try await URLSession.shared.data(for: request)
        let response = try JSONDecoder().decode(VNSingleUseTokenResponse.self, from: data)
        guard !response.token.isEmpty else { throw VoiceNoteSTTError.invalidResponse }
        return response.token
    }

    private func makeWebSocketRequest(token: String, modelID: String) throws -> URLRequest {
        var components = URLComponents()
        components.scheme = "wss"
        components.host = "api.elevenlabs.io"
        components.path = "/v1/speech-to-text/realtime"
        components.queryItems = [
            URLQueryItem(name: "model_id", value: modelID),
            URLQueryItem(name: "token", value: token),
            URLQueryItem(name: "audio_format", value: "pcm_16000"),
            URLQueryItem(name: "commit_strategy", value: "vad"),
            URLQueryItem(name: "vad_silence_threshold_secs", value: "\(vadSilenceThresholdSecs)"),
            URLQueryItem(name: "vad_threshold", value: "\(vadThreshold)"),
            URLQueryItem(name: "include_timestamps", value: "false"),
        ]
        guard let url = components.url else { throw VoiceNoteSTTError.invalidResponse }
        return URLRequest(url: url)
    }

    private func realtimeModelID(from configuredModelID: String) -> String {
        let modelID = configuredModelID.trimmingCharacters(in: .whitespacesAndNewlines)
        if modelID.isEmpty || modelID == "scribe_v2" { return "scribe_v2_realtime" }
        return modelID
    }

    // MARK: - Audio capture

    private func startAudioCapture() throws {
        let avSession = AVAudioSession.sharedInstance()
        try avSession.setCategory(
            .playAndRecord,
            mode: .measurement,
            options: [.defaultToSpeaker, .allowBluetoothHFP]
        )
        try avSession.setPreferredSampleRate(Double(sampleRate))
        try avSession.setPreferredIOBufferDuration(0.1)
        try avSession.setActive(true, options: [])
        didActivateSession = true

        let capture = VNAudioCapture(
            sampleRate: sampleRate,
            packetSink: VNAudioPacketSink(owner: self)
        )
        try capture.start()
        audioCapture = capture
    }

    private func stopAudioCapture() {
        guard let audioCapture else { return }
        audioCapture.stop()
        self.audioCapture = nil
        if didActivateSession {
            try? AVAudioSession.sharedInstance().setCategory(.playback, mode: .spokenAudio)
            try? AVAudioSession.sharedInstance().setActive(true)
            didActivateSession = false
        }
    }

    fileprivate func handleAudioPacket(_ packet: VNAudioPacket) {
        guard shouldAcceptAudio else { return }
        level = packet.level
        enqueueAudio(packet.data)
    }

    private func enqueueAudio(_ data: Data) {
        guard shouldAcceptAudio, shouldQueueAudio, !data.isEmpty else { return }
        pendingAudioChunks.append(data)
        startDrainingAudioQueue()
    }

    private func startDrainingAudioQueue() {
        guard shouldSendAudio, !isSendingAudio else { return }
        isSendingAudio = true
        Task { @MainActor [weak self] in await self?.drainAudioQueue() }
    }

    private func drainAudioQueue() async {
        while shouldSendAudio, !pendingAudioChunks.isEmpty {
            guard let webSocketTask else { pendingAudioChunks.removeAll(); break }
            let data = pendingAudioChunks.removeFirst()
            do {
                try await sendAudio(data, through: webSocketTask)
            } catch {
                realtimeUnavailable(with: error.localizedDescription)
                break
            }
        }
        isSendingAudio = false
        if shouldSendAudio, !pendingAudioChunks.isEmpty { startDrainingAudioQueue() }
    }

    private func sendAudio(_ data: Data, through webSocketTask: URLSessionWebSocketTask) async throws {
        let payload = VNInputAudioChunk(audioBase64: data.base64EncodedString(), sampleRate: sampleRate)
        let encoded = try JSONEncoder().encode(payload)
        guard let text = String(data: encoded, encoding: .utf8) else {
            throw VoiceNoteSTTError.invalidResponse
        }
        try await webSocketTask.send(.string(text))
    }

    // MARK: - Receive loop

    private func receiveLoop() async {
        while !Task.isCancelled, let webSocketTask {
            do {
                let message = try await webSocketTask.receive()
                switch message {
                case .string(let text): handleMessage(text)
                case .data(let data):
                    if let text = String(data: data, encoding: .utf8) { handleMessage(text) }
                @unknown default: break
                }
            } catch {
                if !isClosing { realtimeUnavailable(with: error.localizedDescription) }
                return
            }
        }
    }

    private func handleMessage(_ text: String) {
        guard let data = text.data(using: .utf8) else { return }
        do {
            let event = try JSONDecoder().decode(VNRealtimeEvent.self, from: data)
            switch event.messageType {
            case "partial_transcript":
                partialTranscript = event.text ?? ""
                updateTranscript()
            case "committed_transcript", "committed_transcript_with_timestamps":
                appendCommitted(event.text ?? "")
            case "session_started":
                break
            default:
                if event.messageType.contains("error") {
                    realtimeUnavailable(with: event.errorMessage)
                }
            }
        } catch {
            errorMessage = "Transcript decode error."
        }
    }

    private func appendCommitted(_ text: String) {
        let segment = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !segment.isEmpty else { return }
        committedSegments.append(segment)
        partialTranscript = ""
        updateTranscript()
    }

    private func updateTranscript() {
        var text = committedSegments.joined(separator: " ")
        let partial = partialTranscript.trimmingCharacters(in: .whitespacesAndNewlines)
        if !partial.isEmpty {
            text = text.isEmpty ? partial : "\(text) \(partial)"
        }
        transcript = text
    }

    private func realtimeUnavailable(with message: String) {
        errorMessage = message.isEmpty ? "Live transcription unavailable." : message
        shouldQueueAudio = false
        shouldSendAudio = false
        pendingAudioChunks.removeAll()
        closeSocket()
    }

    private func closeSocket() {
        isClosing = true
        shouldSendAudio = false
        pendingAudioChunks.removeAll()
        receiveTask?.cancel()
        receiveTask = nil
        webSocketTask?.cancel(with: .goingAway, reason: nil)
        webSocketTask = nil
        isSendingAudio = false
    }

    private func resetTranscript() {
        transcript = ""
        partialTranscript = ""
        committedSegments = []
    }
}

// MARK: - Error

enum VoiceNoteSTTError: LocalizedError {
    case micPermissionDenied
    case invalidResponse
    case audio(String)

    var errorDescription: String? {
        switch self {
        case .micPermissionDenied:
            "Microphone access is denied. Go to Settings → Privacy & Security → Microphone."
        case .invalidResponse:
            "Unexpected response from transcription service."
        case .audio(let msg):
            msg
        }
    }
}

// MARK: - Private audio engine

private struct VNAudioPacket: Sendable {
    var data: Data
    var level: Float
}

private final class VNAudioCapture {
    private let sampleRate: Int
    private let packetSink: VNAudioPacketSink
    private var audioEngine: AVAudioEngine?
    private var tapInstalled = false

    init(sampleRate: Int, packetSink: VNAudioPacketSink) {
        self.sampleRate = sampleRate
        self.packetSink = packetSink
    }

    func start() throws {
        guard audioEngine == nil else { return }
        let engine = AVAudioEngine()
        audioEngine = engine

        let input = engine.inputNode
        let inputFormat = input.outputFormat(forBus: 0)

        guard let outputFormat = AVAudioFormat(
            commonFormat: .pcmFormatInt16,
            sampleRate: Double(sampleRate),
            channels: 1,
            interleaved: true
        ) else {
            throw VoiceNoteSTTError.audio("Could not create realtime audio format.")
        }
        guard let converter = AVAudioConverter(from: inputFormat, to: outputFormat) else {
            throw VoiceNoteSTTError.audio("Could not prepare realtime audio conversion.")
        }
        converter.primeMethod = .none

        let tapBufferSize = AVAudioFrameCount(max(1024, min(8192, Int(inputFormat.sampleRate * 0.1))))
        input.installTap(onBus: 0, bufferSize: tapBufferSize, format: inputFormat) { [packetSink, converter, outputFormat] buffer, _ in
            guard let packet = Self.packet(from: buffer, converter: converter, outputFormat: outputFormat) else {
                return
            }
            packetSink.send(packet)
        }
        tapInstalled = true

        do {
            engine.prepare()
            try engine.start()
        } catch {
            stop()
            throw VoiceNoteSTTError.audio("Could not start audio engine: \(error.localizedDescription)")
        }
    }

    func stop() {
        if tapInstalled {
            audioEngine?.inputNode.removeTap(onBus: 0)
            tapInstalled = false
        }
        audioEngine?.stop()
        audioEngine = nil
    }

    private static func packet(
        from buffer: AVAudioPCMBuffer,
        converter: AVAudioConverter,
        outputFormat: AVAudioFormat
    ) -> VNAudioPacket? {
        let inputLevel = level(from: buffer)
        let ratio = outputFormat.sampleRate / buffer.format.sampleRate
        let convertedCapacity = Int(ceil(Double(buffer.frameLength) * ratio)) + 32
        let capacity = max(Int(buffer.frameLength), convertedCapacity, 1)
        guard let outputBuffer = AVAudioPCMBuffer(
            pcmFormat: outputFormat,
            frameCapacity: AVAudioFrameCount(capacity)
        ) else { return nil }

        let inputProvider = VNConverterInputProvider(buffer: buffer)
        var conversionError: NSError?
        let status = converter.convert(to: outputBuffer, error: &conversionError) { _, inputStatus in
            inputProvider.provideInput(status: inputStatus)
        }

        guard outputBuffer.frameLength > 0 else {
            switch status {
            case .haveData, .inputRanDry, .endOfStream, .error:
                return VNAudioPacket(data: Data(), level: inputLevel)
            @unknown default:
                return VNAudioPacket(data: Data(), level: inputLevel)
            }
        }

        let audioBuffer = outputBuffer.audioBufferList.pointee.mBuffers
        let bytesPerFrame = Int(outputFormat.streamDescription.pointee.mBytesPerFrame)
        let byteCount = Int(outputBuffer.frameLength) * bytesPerFrame
        guard let bytes = audioBuffer.mData, byteCount > 0 else { return nil }

        let availableByteCount = min(byteCount, Int(audioBuffer.mDataByteSize))
        let data = Data(bytes: bytes, count: availableByteCount)
        return VNAudioPacket(data: data, level: max(inputLevel, level(from: data)))
    }

    private static func level(from buffer: AVAudioPCMBuffer) -> Float {
        let frameLength = Int(buffer.frameLength)
        guard frameLength > 0 else { return 0 }

        if let channels = buffer.floatChannelData {
            let channelCount = max(1, Int(buffer.format.channelCount))
            var sum: Float = 0
            var count = 0
            for channel in 0..<channelCount {
                let samples = channels[channel]
                for frame in 0..<frameLength {
                    let s = samples[frame]
                    sum += s * s
                    count += 1
                }
            }
            guard count > 0 else { return 0 }
            return min(1, sqrt(sum / Float(count)) * 8)
        }

        if let channels = buffer.int16ChannelData {
            let channelCount = max(1, Int(buffer.format.channelCount))
            var sum: Float = 0
            var count = 0
            for channel in 0..<channelCount {
                let samples = channels[channel]
                for frame in 0..<frameLength {
                    let normalized = Float(samples[frame]) / Float(Int16.max)
                    sum += normalized * normalized
                    count += 1
                }
            }
            guard count > 0 else { return 0 }
            return min(1, sqrt(sum / Float(count)) * 8)
        }
        return 0
    }

    private static func level(from data: Data) -> Float {
        data.withUnsafeBytes { rawBuffer in
            let samples = rawBuffer.bindMemory(to: Int16.self)
            guard !samples.isEmpty else { return 0 }
            var sum: Float = 0
            for sample in samples {
                let normalized = Float(sample) / Float(Int16.max)
                sum += normalized * normalized
            }
            return min(1, sqrt(sum / Float(samples.count)) * 8)
        }
    }
}

private final class VNConverterInputProvider: @unchecked Sendable {
    private let buffer: AVAudioPCMBuffer
    private var didProvideInput = false

    init(buffer: AVAudioPCMBuffer) { self.buffer = buffer }

    func provideInput(status: UnsafeMutablePointer<AVAudioConverterInputStatus>) -> AVAudioBuffer? {
        if didProvideInput {
            status.pointee = .noDataNow
            return nil
        }
        didProvideInput = true
        status.pointee = .haveData
        return buffer
    }
}

private final class VNAudioPacketSink: @unchecked Sendable {
    private weak var owner: VoiceNoteRealtimeSTT?

    @MainActor
    init(owner: VoiceNoteRealtimeSTT) { self.owner = owner }

    func send(_ packet: VNAudioPacket) {
        let owner = owner
        Task { @MainActor in owner?.handleAudioPacket(packet) }
    }
}

// MARK: - API types

private struct VNInputAudioChunk: Encodable {
    var messageType = "input_audio_chunk"
    var audioBase64: String
    var sampleRate: Int

    enum CodingKeys: String, CodingKey {
        case messageType = "message_type"
        case audioBase64 = "audio_base_64"
        case sampleRate = "sample_rate"
    }
}

private struct VNSingleUseTokenResponse: Decodable {
    var token: String
}

private struct VNRealtimeEvent: Decodable {
    var messageType: String
    var text: String?
    var error: String?
    var message: String?
    var detail: String?

    var errorMessage: String {
        error ?? message ?? detail ?? "Realtime transcription failed."
    }

    enum CodingKeys: String, CodingKey {
        case messageType = "message_type"
        case text, error, message, detail
    }
}
