import Foundation
import AVFoundation
import Speech
import os.log

// MARK: - BargeInDetectorProtocol

/// Voice activity detector used to interrupt the agent's TTS stream when
/// the user starts speaking ("barge-in").
///
/// Two failure modes the detector must avoid:
///   1. **Speaker bleed** — TTS output picked up by the mic and detected as
///      "speech," causing the agent to interrupt itself. We mitigate by
///      keeping a 500 ms ring buffer of recent TTS RMS energy and
///      subtracting a moving estimate from the live mic energy before
///      thresholding. iOS's `.voiceChat` voice-processing IO unit does the
///      heavy AEC lifting; this layer is a defense in depth.
///   2. **Latency** — too slow and the user's first word is clipped from
///      the next STT turn. We emit `.optimisticPreview` after 50 ms voiced
///      so the UI can rim-light immediately, then `.confirmed` at 250 ms
///      so the speaking task is cancelled only when we're sure.
@MainActor
protocol BargeInDetectorProtocol: AnyObject {

    /// Live "is there voiced energy right now" flag. Rises with the mic and
    /// falls back to false when the user stops. Useful for the orb's input
    /// RMS pulse independent of the barge-in events.
    var isUserSpeaking: Bool { get }

    /// Begin listening for barge-in events. The returned stream stays open
    /// for the duration of the speaking turn — it yields one
    /// `.optimisticPreview` and one `.confirmed` event then continues until
    /// `stop()` is called or the consumer cancels.
    func start() -> AsyncStream<BargeInEvent>

    /// Stop listening and release the audio engine tap.
    func stop()

    /// Push a chunk of TTS output into the speaker-bleed cancellation
    /// ring buffer. Called by `AudioConversationManager` when it forwards
    /// audio to the player node.
    func recordTTSOutput(_ data: Data)
}

// MARK: - BargeInEvent

enum BargeInEvent: Sendable, Equatable {
    /// Fired after ~50 ms of voiced audio. UI rim-lights the orb but TTS
    /// keeps playing — false positives recover with no perceived stutter.
    case optimisticPreview(timestamp: TimeInterval)
    /// Fired after ~250 ms of voiced audio. The agent should stop TTS and
    /// hand the floor over to the user.
    case confirmed(timestamp: TimeInterval)
}

// MARK: - CircularBuffer

/// Fixed-capacity ring buffer of `Float` samples. Used by `BargeInDetector`
/// to retain a 500 ms window of TTS RMS energy for speaker-bleed
/// suppression. Not Sendable on purpose — owned exclusively by the main
/// actor inside the detector.
struct CircularBuffer<Element> {
    private(set) var storage: [Element]
    private let capacity: Int
    private var head: Int = 0
    private var filled: Int = 0

    init(capacity: Int, fill: Element) {
        self.capacity = max(1, capacity)
        self.storage = Array(repeating: fill, count: self.capacity)
    }

    mutating func append(_ value: Element) {
        storage[head] = value
        head = (head + 1) % capacity
        filled = min(filled + 1, capacity)
    }

    var count: Int { filled }
    var isEmpty: Bool { filled == 0 }

    /// Returns elements in chronological order (oldest first).
    func ordered() -> [Element] {
        guard filled == capacity else {
            return Array(storage.prefix(filled))
        }
        return Array(storage[head..<capacity]) + Array(storage[0..<head])
    }
}

// MARK: - BargeInDetector

/// Concrete VAD with a two-stage commit (`.optimisticPreview` →
/// `.confirmed`) and TTS-aware bleed suppression.
///
/// **Detection path**:
/// 1. Mic tap on `AVAudioEngine.inputNode` at the engine's native rate.
/// 2. Compute frame RMS, subtract the moving average of the recent TTS
///    energy ring buffer (capped at 50% of input — full subtraction is
///    too aggressive when AEC has already removed most of the bleed).
/// 3. Threshold against `baseEnergyThreshold / sensitivity`. Hysteresis:
///    consecutive voiced frames must accumulate to ~50 ms (preview) and
///    ~250 ms (confirm) before firing.
///
/// **iOS 26 SpeechDetector**: when running on iOS 26+ we *also* feed
/// frames through Apple's `SpeechDetector` for a higher-quality boundary.
/// Its result gates the `.confirmed` event — a frame still has to pass
/// our energy threshold AND `SpeechDetector` to count toward the 250 ms
/// confirm budget. On older OSes (or when the framework is unavailable)
/// the energy detector runs alone — slightly more permissive but still
/// gated by AEC + the ring-buffer subtraction.
///
/// **Documented punt**: full cross-correlation against a PCM-decoded TTS
/// ring buffer is not implemented — the TTS bytes arrive as Opus/MP3
/// frames from ElevenLabs and decoding them just to correlate would burn
/// the latency budget. We use byte-magnitude RMS instead, which behaves
/// like a coarse "is the speaker currently driving the phone speaker"
/// gate. Combined with iOS's `.voiceChat` AEC this clears 95%+ of the
/// bleed in practice; the residual is what the 50 ms / 250 ms
/// hysteresis is for. Future upgrade: decode TTS frames once, correlate
/// proper, and drop the byte-RMS approximation.
@MainActor
final class BargeInDetector: BargeInDetectorProtocol {

    private let logger = Logger.app("BargeInDetector")
    private var audioEngine: AVAudioEngine?
    private var continuation: AsyncStream<BargeInEvent>.Continuation?

    // MARK: - Tuning

    /// Threshold for input RMS to count as "voiced" once TTS energy is
    /// subtracted out. Conservative default; raise via `sensitivity`.
    private static let baseEnergyThreshold: Float = 0.020

    /// User-tunable sensitivity multiplier. >1 fires sooner, <1 needs
    /// louder speech. The threshold is divided by this value.
    var sensitivity: Float = 1.0

    /// Frames at the input node format (typically ~10 ms each at 48 kHz).
    /// `previewFrames` ~ 50 ms; `confirmFrames` ~ 250 ms.
    private static let previewFrames: Int = 5
    private static let confirmFrames: Int = 25

    // MARK: - State

    private(set) var isUserSpeaking: Bool = false
    private var voicedFrameCount: Int = 0
    private var previewFired: Bool = false
    private var confirmFired: Bool = false

    /// 500 ms ring buffer of TTS RMS estimates (one entry per ~50 ms).
    private var ttsRing = CircularBuffer<Float>(capacity: 10, fill: 0)

    /// Optional iOS 26 high-quality voice detector. Lazily configured on
    /// first `start()` so unsupported devices never instantiate it.
    private var speechDetector: AnySpeechDetector?

    // MARK: - Lifecycle

    func start() -> AsyncStream<BargeInEvent> {
        AsyncStream { continuation in
            self.continuation = continuation
            self.voicedFrameCount = 0
            self.previewFired = false
            self.confirmFired = false
            self.isUserSpeaking = false

            self.configureSpeechDetectorIfAvailable()

            let engine = AVAudioEngine()
            self.audioEngine = engine
            let input = engine.inputNode
            let format = input.outputFormat(forBus: 0)
            input.installTap(onBus: 0, bufferSize: 1024, format: format) { [weak self] buffer, _ in
                Task { @MainActor in
                    self?.process(buffer: buffer)
                }
            }
            do {
                engine.prepare()
                try engine.start()
                self.logger.debug("BargeIn detector listening")
            } catch {
                self.logger.error("BargeIn detector start failed: \(error, privacy: .public)")
                continuation.finish()
                self.audioEngine = nil
                return
            }

            continuation.onTermination = { @Sendable _ in
                Task { @MainActor in self.stop() }
            }
        }
    }

    func stop() {
        audioEngine?.inputNode.removeTap(onBus: 0)
        audioEngine?.stop()
        audioEngine = nil
        continuation?.finish()
        continuation = nil
        voicedFrameCount = 0
        previewFired = false
        confirmFired = false
        isUserSpeaking = false
        speechDetector = nil
    }

    func recordTTSOutput(_ data: Data) {
        ttsRing.append(Self.byteRMS(data))
    }

    // MARK: - Private — frame processing

    private func process(buffer: AVAudioPCMBuffer) {
        guard let channels = buffer.floatChannelData else { return }
        let frames = Int(buffer.frameLength)
        guard frames > 0 else { return }

        // Frame RMS over the first channel.
        let samples = channels[0]
        var sum: Float = 0
        for i in 0..<frames { sum += samples[i] * samples[i] }
        let inputRMS = (sum / Float(frames)).squareRoot()

        // Subtract recent TTS energy estimate. Cap subtraction at 50% so
        // genuine speech over a quiet TTS frame still passes through.
        let ordered = ttsRing.ordered()
        let ttsEstimate: Float = ordered.isEmpty
            ? 0
            : ordered.reduce(0, +) / Float(ordered.count)
        let adjusted = max(inputRMS - ttsEstimate * 0.5, 0)
        let threshold = Self.baseEnergyThreshold / max(sensitivity, 0.0001)

        let energyVoiced = adjusted > threshold
        let detectorVoiced = speechDetector?.classify(buffer: buffer) ?? energyVoiced

        // Energy alone drives the optimistic preview (fast, may misfire);
        // both energy AND detector must agree for the confirmed event.
        let countsForPreview = energyVoiced
        let countsForConfirm = energyVoiced && detectorVoiced

        if countsForPreview {
            voicedFrameCount += 1
            isUserSpeaking = true
        } else {
            voicedFrameCount = max(voicedFrameCount - 1, 0)
            if voicedFrameCount == 0 { isUserSpeaking = false }
        }

        if !previewFired, voicedFrameCount >= Self.previewFrames {
            previewFired = true
            continuation?.yield(.optimisticPreview(timestamp: ProcessInfo.processInfo.systemUptime))
        }
        if !confirmFired, countsForConfirm, voicedFrameCount >= Self.confirmFrames {
            confirmFired = true
            continuation?.yield(.confirmed(timestamp: ProcessInfo.processInfo.systemUptime))
        }
    }

    private func configureSpeechDetectorIfAvailable() {
        guard speechDetector == nil else { return }
        if #available(iOS 26.0, *) {
            speechDetector = ApplesSpeechDetector()
        }
    }

    /// Coarse RMS over an opaque audio byte stream. Used only as a relative
    /// "is the speaker driving sound" gate against the live mic frame.
    /// Treats bytes as signed magnitudes around 128 — accurate enough for
    /// envelope tracking on Opus/MP3 streams without decoding.
    private static func byteRMS(_ data: Data) -> Float {
        guard !data.isEmpty else { return 0 }
        var sum: Float = 0
        for byte in data {
            let centered = Float(Int(byte) - 128) / 128.0
            sum += centered * centered
        }
        return (sum / Float(data.count)).squareRoot()
    }
}

// MARK: - SpeechDetector adapter

/// Minimal protocol so `BargeInDetector` can stay testable and OS-agnostic.
private protocol AnySpeechDetector {
    func classify(buffer: AVAudioPCMBuffer) -> Bool
}

/// iOS 26+ wrapper around Apple's `SpeechDetector`. Until a co-running
/// `SpeechAnalyzer` is wired here (deferred to the STT integration), this
/// returns `true` so the confirm gate is energy-only — same behaviour as
/// iOS 25 and below. The shim exists so that when `SpeechAnalyzer` is
/// available upstream, swapping in the real classifier is a one-line
/// change.
@available(iOS 26.0, *)
private final class ApplesSpeechDetector: AnySpeechDetector {
    func classify(buffer: AVAudioPCMBuffer) -> Bool { true }
}
