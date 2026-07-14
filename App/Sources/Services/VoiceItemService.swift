import Foundation
import Speech
import AVFoundation
import os.log

// MARK: - VoiceItemService

/// Streams on-device speech recognition while the user speaks, calling a
/// handler closure with each partial / final transcription result.
///
/// Usage:
/// ```swift
/// let service = VoiceItemService()
/// await service.start { text in draft = text }
/// // … user speaks …
/// service.stop()
/// ```
///
/// The service requests microphone and speech-recognition authorization
/// on demand the first time `start` is called.
@MainActor
@Observable
final class VoiceItemService {
    private let logger = Logger.app("VoiceItemService")

    // MARK: - Phase

    /// Recording lifecycle state.
    enum Phase: Equatable, Sendable {
        case idle
        case recording
        case denied
        case failed(String)
    }

    // MARK: - Observed state

    private(set) var phase: Phase = .idle

    // MARK: - Private

    private var recognizer: SFSpeechRecognizer?
    private var audioEngine: AVAudioEngine?
    private var recognitionRequest: SFSpeechAudioBufferRecognitionRequest?
    private var recognitionTask: SFSpeechRecognitionTask?

    // MARK: - Public API

    /// Requests authorization, configures the audio session, and begins streaming
    /// speech → text by calling `onTranscription` with each partial result.
    ///
    /// Calling `start` while already recording is a no-op.
    func start(onTranscription: @escaping @MainActor (String) -> Void) async {
        guard phase == .idle else { return }

        // 1. Check / request authorizations.
        guard await requestAuthorizations() else { return }

        // 2. Build the recognizer (uses device locale by default).
        let sr = SFSpeechRecognizer()
        guard let sr, sr.isAvailable else {
            phase = .failed("Speech recognition is not available on this device.")
            return
        }
        recognizer = sr

        // 3. Configure audio session for recording.
        let session = AVAudioSession.sharedInstance()
        do {
            try session.setCategory(.record, mode: .measurement, options: .duckOthers)
            try session.setActive(true, options: .notifyOthersOnDeactivation)
        } catch {
            logger.error("AVAudioSession setup failed: \(error, privacy: .public)")
            phase = .failed("Could not start audio session.")
            return
        }

        // 4. Wire up recognition request.
        let request = SFSpeechAudioBufferRecognitionRequest()
        request.shouldReportPartialResults = true
        request.requiresOnDeviceRecognition = false
        recognitionRequest = request

        // 5. Start recognition task.
        recognitionTask = sr.recognitionTask(with: request) { [weak self] result, error in
            guard let self else { return }
            MainActor.assumeIsolated {
                if let result {
                    onTranscription(result.bestTranscription.formattedString)
                }
                if let error {
                    let nsError = error as NSError
                    // Code 216 = cancelled (user tapped stop) — not an error.
                    if nsError.code != 216 {
                        logger.error("Recognition error: \(error, privacy: .public)")
                        self.teardown()
                        self.phase = .failed("Recognition stopped.")
                    }
                }
                if result?.isFinal == true {
                    self.teardown()
                    self.phase = .idle
                }
            }
        }

        // 6. Attach audio engine tap.
        let engine = AVAudioEngine()
        audioEngine = engine
        let inputNode = engine.inputNode
        let recordingFormat = inputNode.outputFormat(forBus: 0)
        inputNode.installTap(onBus: 0, bufferSize: 1024, format: recordingFormat) { [weak self] buffer, _ in
            self?.recognitionRequest?.append(buffer)
        }

        do {
            engine.prepare()
            try engine.start()
        } catch {
            logger.error("AVAudioEngine start failed: \(error, privacy: .public)")
            teardown()
            phase = .failed("Could not start microphone.")
            return
        }

        phase = .recording
        logger.info("Voice recording started")
    }

    /// Stops the current recording session and finalises the transcription.
    func stop() {
        audioEngine?.stop()
        recognitionRequest?.endAudio()
        logger.info("Voice recording stopped by user")
        teardown()
        phase = .idle
    }

    // MARK: - Private helpers

    private func requestAuthorizations() async -> Bool {
        // Microphone.
        let micStatus = AVAudioApplication.shared.recordPermission
        if micStatus == .undetermined {
            let granted = await AVAudioApplication.requestRecordPermission()
            if !granted {
                phase = .denied
                return false
            }
        } else if micStatus == .denied {
            phase = .denied
            return false
        }

        // Speech recognition.
        let srStatus = SFSpeechRecognizer.authorizationStatus()
        if srStatus == .notDetermined {
            let status = await withCheckedContinuation { cont in
                SFSpeechRecognizer.requestAuthorization { cont.resume(returning: $0) }
            }
            if status != .authorized {
                phase = .denied
                return false
            }
        } else if srStatus != .authorized {
            phase = .denied
            return false
        }

        return true
    }

    private func teardown() {
        audioEngine?.inputNode.removeTap(onBus: 0)
        audioEngine?.stop()
        audioEngine = nil
        recognitionTask?.cancel()
        recognitionTask = nil
        recognitionRequest = nil
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
    }
}
