import AVFoundation
import Foundation
import Speech
import os.log

// MARK: - Voice capability — `nmp.voice.capability`
//
// iOS half of the voice capability defined in
// `apps/nmp-app-podcast/src/capability/voice.rs` (M8.A + feature #42).
// Translates `VoiceCommand` JSON into on-device `SFSpeechRecognizer`
// (STT) and `AVSpeechSynthesizer` (TTS) operations, and pushes
// `VoiceReport` JSON back to Rust through an asynchronous `sendReport`
// channel mirroring `AudioCapability`.
//
// Doctrine:
//   D6 — errors never throw across the boundary. Permission denials,
//        recognizer unavailability, audio-session preempts all surface
//        as `VoiceReport.Error` or `VoiceReport.Failed`.
//   D7 — this capability *executes and reports*. Barge-in policy
//        (cancel speech when partial transcript arrives) is *currently*
//        co-located in `VoiceCapability+Barge.swift` because the M8.A
//        Rust loop is still a stub; once `podcast-voice::manager` lands
//        in a future milestone, the policy moves to Rust and this
//        executor will only honour explicit `Stop` commands.
//
// File-length budget: dispatch + command translation core. Barge-in
// extension + wire vocabulary live in sibling files
// (`VoiceCapability+Barge.swift`, `VoiceCapability+Wire.swift`).

/// `SFSpeechRecognizer` + `AVSpeechSynthesizer`-backed executor for the
/// voice capability.
///
/// Single-instance, owned by `PodcastCapabilities`. State is the live
/// audio engine + recognition task + synthesizer; every policy decision
/// (when to start listening, when to stop on silence, which voice to
/// fall back to) lives or will live in Rust.
@MainActor
final class VoiceCapability: NSObject {
    static let namespace = "nmp.voice.capability"

    private let logger = Logger(subsystem: "io.f7z.podcast", category: "VoiceCapability")

    // ── STT runtime ──────────────────────────────────────────────────────
    private let audioEngine = AVAudioEngine()
    private var recognizer: SFSpeechRecognizer?
    private var recognitionRequest: SFSpeechAudioBufferRecognitionRequest?
    private var recognitionTask: SFSpeechRecognitionTask?
    /// Buffered final transcript text for the active listening turn.
    /// Cleared on `StartListening`; emitted on `StopListening` /
    /// recognizer final result.
    var lastFinalText: String = ""

    // ── TTS runtime ──────────────────────────────────────────────────────
    let synthesizer = AVSpeechSynthesizer()
    /// `request_id` of the currently-speaking `Speak`, mirrored back in
    /// `SpeakingStarted` / `SpeakingFinished` / `Failed`. `nil` between
    /// turns.
    var activeSpeakRequestID: String?
    /// Voice id (locale identifier) the Rust side most recently picked
    /// via `SetVoice`. `nil` falls back to the device default.
    var activeVoiceID: String?

    // ── Out-of-band event sink to Rust ──────────────────────────────────
    /// Defaults to a no-op so the executor is exercisable from tests /
    /// previews; the bridge installs the real channel via `attach`.
    private var sendReport: (String) -> Void = { _ in }

    private var started: Bool = false
    private var synthesizerDelegate: SpeechSynthesizerDelegate?

    // MARK: Lifecycle

    override init() {
        super.init()
        let delegate = SpeechSynthesizerDelegate(owner: self)
        self.synthesizerDelegate = delegate
        synthesizer.delegate = delegate
    }

    /// Idempotent. Marks the executor active and installs the report
    /// channel. Safe to call on every app foreground.
    func attach(sendReport: @escaping (String) -> Void) {
        self.sendReport = sendReport
        start()
    }

    func start() {
        guard !started else { return }
        started = true
    }

    func stop() {
        started = false
        tearDownRecognition(reason: nil)
        synthesizer.stopSpeaking(at: .immediate)
    }

    // MARK: - Command entry points

    /// Decode a `CapabilityRequest` JSON envelope and execute the
    /// contained `VoiceCommand`. Honors D6: malformed input degrades to
    /// an error envelope, never throws.
    @discardableResult
    func handleJSON(_ requestJSON: String) -> String {
        guard
            let data = requestJSON.data(using: .utf8),
            let request = try? JSONDecoder().decode(CapabilityRequest.self, from: data)
        else {
            return errorEnvelope(correlationID: "", message: "malformed-request")
        }
        guard
            let payload = request.payloadJSON.data(using: .utf8),
            let command = try? JSONDecoder().decode(VoiceCommand.self, from: payload)
        else {
            return errorEnvelope(correlationID: request.correlationID, message: "malformed-payload")
        }
        execute(command)
        return okEnvelope(correlationID: request.correlationID)
    }

    /// Direct command entry — used by tests and any synchronous caller.
    func execute(_ command: VoiceCommand) {
        switch command {
        case .startListening:
            startListening()
        case .stopListening:
            tearDownRecognition(reason: .userStop)
        case let .speak(text, voiceID, requestID):
            speak(text: text, voiceID: voiceID, requestID: requestID)
        case .stop:
            stopSpeaking()
        case let .setVoice(voiceID):
            activeVoiceID = voiceID.isEmpty ? nil : voiceID
        }
    }

    // MARK: - STT (SFSpeechRecognizer)

    private func startListening() {
        guard recognitionTask == nil else { return }
        requestPermissions { [weak self] granted, message in
            guard let self else { return }
            guard granted else {
                self.emit(.error(message: message ?? "voice permission denied"))
                return
            }
            self.beginRecognitionTask()
        }
    }

    private func beginRecognitionTask() {
        let recognizer = SFSpeechRecognizer() ?? SFSpeechRecognizer(locale: Locale(identifier: "en-US"))
        guard let recognizer, recognizer.isAvailable else {
            emit(.error(message: "speech recognizer unavailable"))
            return
        }
        self.recognizer = recognizer
        do {
            try configureAudioSessionForRecognition()
        } catch {
            emit(.error(message: "audio session: \(error.localizedDescription)"))
            return
        }
        let request = SFSpeechAudioBufferRecognitionRequest()
        request.shouldReportPartialResults = true
        self.recognitionRequest = request
        lastFinalText = ""

        let inputNode = audioEngine.inputNode
        let format = inputNode.outputFormat(forBus: 0)
        inputNode.removeTap(onBus: 0)
        inputNode.installTap(onBus: 0, bufferSize: 1024, format: format) { [weak request] buffer, _ in
            request?.append(buffer)
        }
        audioEngine.prepare()
        do {
            try audioEngine.start()
        } catch {
            tearDownRecognition(reason: .startFailed)
            emit(.error(message: "audio engine: \(error.localizedDescription)"))
            return
        }
        recognitionTask = recognizer.recognitionTask(with: request) { [weak self] result, error in
            Task { @MainActor [weak self] in
                self?.handleRecognition(result: result, error: error)
            }
        }
        emit(.listeningStarted)
    }

    private func handleRecognition(result: SFSpeechRecognitionResult?, error: Error?) {
        if let result {
            let text = result.bestTranscription.formattedString
            lastFinalText = text
            if result.isFinal {
                emit(.transcriptFinal(text: text))
                tearDownRecognition(reason: .finalResult)
                return
            }
            emit(.transcriptPartial(text: text))
            // Barge-in policy lives in `VoiceCapability+Barge.swift`.
            notifyPartialForBargeIn(text: text)
        }
        if let error {
            emit(.error(message: error.localizedDescription))
            tearDownRecognition(reason: .recognizerError)
        }
    }

    /// Tear down the live recognition session. `reason` controls whether
    /// to flush a final transcript first (`.userStop` emits the buffered
    /// text; `.finalResult` already emitted; `.startFailed` /
    /// `.recognizerError` skip the flush).
    func tearDownRecognition(reason: TeardownReason?) {
        audioEngine.inputNode.removeTap(onBus: 0)
        if audioEngine.isRunning {
            audioEngine.stop()
        }
        recognitionRequest?.endAudio()
        recognitionTask?.cancel()
        recognitionRequest = nil
        recognitionTask = nil
        if reason == .userStop, !lastFinalText.isEmpty {
            emit(.transcriptFinal(text: lastFinalText))
        }
        lastFinalText = ""
        if reason != nil {
            emit(.listeningStopped)
        }
    }

    enum TeardownReason {
        case userStop
        case finalResult
        case startFailed
        case recognizerError
    }

    // MARK: - TTS (AVSpeechSynthesizer)

    private func speak(text: String, voiceID: String?, requestID: String) {
        let utterance = AVSpeechUtterance(string: text)
        if let voiceID, !voiceID.isEmpty {
            utterance.voice = AVSpeechSynthesisVoice(identifier: voiceID)
                ?? AVSpeechSynthesisVoice(language: voiceID)
            activeVoiceID = voiceID
        } else if let activeVoiceID {
            utterance.voice = AVSpeechSynthesisVoice(identifier: activeVoiceID)
                ?? AVSpeechSynthesisVoice(language: activeVoiceID)
        }
        activeSpeakRequestID = requestID
        synthesizer.speak(utterance)
    }

    func stopSpeaking() {
        if synthesizer.isSpeaking {
            synthesizer.stopSpeaking(at: .immediate)
        } else {
            // No active utterance — still report so the kernel can drop
            // any speaking flag idempotently.
            emit(.stopped)
        }
    }

    // MARK: - Permission

    private func requestPermissions(_ done: @MainActor @escaping (Bool, String?) -> Void) {
        SFSpeechRecognizer.requestAuthorization { status in
            // `done` is captured as a @MainActor closure; hop through Task
            // so the inner Sendable closure boundary doesn't try to send
            // a non-Sendable function value across actors.
            Task { @MainActor in
                guard status == .authorized else {
                    done(false, "speech recognition not authorized")
                    return
                }
                AVAudioApplication.requestRecordPermission { granted in
                    Task { @MainActor in
                        done(granted, granted ? nil : "microphone access denied")
                    }
                }
            }
        }
    }

    private func configureAudioSessionForRecognition() throws {
        let session = AVAudioSession.sharedInstance()
        try session.setCategory(.playAndRecord, mode: .spokenAudio, options: [.duckOthers, .defaultToSpeaker])
        try session.setActive(true, options: .notifyOthersOnDeactivation)
    }

    // MARK: - Report emit helpers

    func emit(_ report: VoiceReport) {
        guard let json = report.jsonString() else { return }
        sendReport(json)
    }

    // MARK: - Envelope encoding

    private func okEnvelope(correlationID: String) -> String {
        let env = CapabilityEnvelope(
            namespace: Self.namespace,
            correlationID: correlationID,
            resultJSON: "{\"status\":\"ok\"}")
        return Self.encode(env) ?? "{}"
    }

    private func errorEnvelope(correlationID: String, message: String) -> String {
        let payload = "{\"status\":\"error\",\"message\":\"\(message)\"}"
        let env = CapabilityEnvelope(
            namespace: Self.namespace,
            correlationID: correlationID,
            resultJSON: payload)
        return Self.encode(env) ?? "{}"
    }

    private static func encode<T: Encodable>(_ value: T) -> String? {
        guard let data = try? JSONEncoder().encode(value) else { return nil }
        return String(data: data, encoding: .utf8)
    }
}

// `SpeechSynthesizerDelegate` lives in
// `VoiceCapability+Synthesizer.swift` (AGENTS.md 300-LOC soft limit).
