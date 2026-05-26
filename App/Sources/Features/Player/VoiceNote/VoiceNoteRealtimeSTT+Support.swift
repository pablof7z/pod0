import AVFoundation
import Foundation

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

// MARK: - Audio engine helpers

struct VNAudioPacket: Sendable {
    var data: Data
    var level: Float
}

final class VNAudioCapture {
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

final class VNConverterInputProvider: @unchecked Sendable {
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

final class VNAudioPacketSink: @unchecked Sendable {
    private weak var owner: VoiceNoteRealtimeSTT?

    @MainActor
    init(owner: VoiceNoteRealtimeSTT) { self.owner = owner }

    func send(_ packet: VNAudioPacket) {
        let owner = owner
        Task { @MainActor in owner?.handleAudioPacket(packet) }
    }
}

// MARK: - API types

struct VNInputAudioChunk: Encodable {
    var messageType = "input_audio_chunk"
    var audioBase64: String
    var sampleRate: Int

    enum CodingKeys: String, CodingKey {
        case messageType = "message_type"
        case audioBase64 = "audio_base_64"
        case sampleRate = "sample_rate"
    }
}

struct VNSingleUseTokenResponse: Decodable {
    var token: String
}

struct VNRealtimeEvent: Decodable {
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
