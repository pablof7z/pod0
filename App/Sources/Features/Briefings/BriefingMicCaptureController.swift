import AVFoundation
import Foundation
import Observation
import os.log
import Speech

// MARK: - BriefingMicCaptureController

/// Hold-to-talk capture controller for the briefing player's barge-in flow
/// (UX-08 §5 *Hold-to-pause-and-ask*).
///
/// Flow when the user presses and holds the mic glyph:
///   1. `requestPermission()` — mic + speech-recognition. Falls back to
///      `.denied` if either is refused. The caller renders the typed sheet
///      instead.
///   2. `start(onTranscription:)` — flips `AudioSessionCoordinator` to
///      `.duckedForVoice`, spins up an `AVAudioEngine` tap, streams partial
///      transcripts back via the closure.
///   3. `stop()` — tears the engine down, returns the audio session to
///      `.briefingPlayback` so the briefing keeps playing.
///
/// The controller is intentionally narrow: no agent-answer playback, no UI.
/// `BriefingPlayerView` owns the gesture, the chrome, and the handoff into
/// `BriefingPlayerEngine.beginBranch / endBranch`.
@MainActor
@Observable
final class BriefingMicCaptureController {

    // MARK: - Phase

    enum Phase: Equatable, Sendable {
        case idle
        case requestingPermission
        case recording
        /// Permission was denied (mic or speech). Caller should fall back to
        /// the typed prompt sheet.
        case denied
        case failed(String)
    }

    // MARK: - Observed state

    private(set) var phase: Phase = .idle

    /// Latest live transcript. Cleared on every `start`.
    private(set) var liveTranscript: String = ""

    // MARK: - Private

    private let logger = Logger.app("BriefingMicCapture")
    private var recognizer: SFSpeechRecognizer?
    private var audioEngine: AVAudioEngine?
    private var recognitionRequest: SFSpeechAudioBufferRecognitionRequest?
    private var recognitionTask: SFSpeechRecognitionTask?

    // MARK: - Public API

    /// Asks for mic + speech recognition authorization. Updates `phase` on
    /// denial. Cheap to call repeatedly — already-granted authorizations
    /// short-circuit.
    @discardableResult
    func requestPermission() async -> Bool {
        phase = .requestingPermission

        // Microphone.
        let micStatus = AVAudioApplication.shared.recordPermission
        if micStatus == .undetermined {
            let granted = await AVAudioApplication.requestRecordPermission()
            if !granted { phase = .denied; return false }
        } else if micStatus == .denied {
            phase = .denied; return false
        }

        // Speech recognition.
        let srStatus = SFSpeechRecognizer.authorizationStatus()
        if srStatus == .notDetermined {
            let status = await withCheckedContinuation { cont in
                SFSpeechRecognizer.requestAuthorization { cont.resume(returning: $0) }
            }
            if status != .authorized { phase = .denied; return false }
        } else if srStatus != .authorized {
            phase = .denied; return false
        }

        phase = .idle
        return true
    }

    /// Begin recording. The audio session is duck-and-record; the underlying
    /// briefing audio drops 12 dB but keeps playing so the user can talk
    /// over it (the *barge-in* contract — UX-08 §4).
    ///
    /// - Parameter onTranscription: Called on the main actor with each
    ///   partial transcription. The same string is mirrored on
    ///   `liveTranscript` for SwiftUI binding ergonomics.
    func start(onTranscription: @escaping @MainActor (String) -> Void) async {
        guard phase == .idle else { return }
        guard await requestPermission() else { return }

        liveTranscript = ""

        // Flip the shared audio session into the AEC-enabled duck mode.
        do {
            try AudioSessionCoordinator.shared.activate(.duckedForVoice)
        } catch {
            logger.error("Audio session duck failed: \(error.localizedDescription, privacy: .public)")
            phase = .failed("Could not duck audio for mic.")
            return
        }

        let sr = SFSpeechRecognizer()
        guard let sr, sr.isAvailable else {
            phase = .failed("Speech recognition is not available.")
            return
        }
        recognizer = sr

        let request = SFSpeechAudioBufferRecognitionRequest()
        request.shouldReportPartialResults = true
        request.requiresOnDeviceRecognition = false
        recognitionRequest = request

        recognitionTask = sr.recognitionTask(with: request) { [weak self] result, error in
            guard let self else { return }
            MainActor.assumeIsolated {
                if let result {
                    let text = result.bestTranscription.formattedString
                    self.liveTranscript = text
                    onTranscription(text)
                }
                if let error {
                    let nsError = error as NSError
                    if nsError.code != 216 { // 216 = cancelled, not an error
                        self.logger.error("Recognition error: \(error.localizedDescription, privacy: .public)")
                        self.teardown(restoreSession: true)
                        self.phase = .failed("Recognition stopped.")
                    }
                }
                if result?.isFinal == true {
                    self.teardown(restoreSession: true)
                    self.phase = .idle
                }
            }
        }

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
            logger.error("AVAudioEngine start failed: \(error.localizedDescription, privacy: .public)")
            teardown(restoreSession: true)
            phase = .failed("Could not start microphone.")
            return
        }

        phase = .recording
        logger.info("Briefing mic capture started")
    }

    /// Stop recording. Returns the finalised transcript (whatever was on
    /// `liveTranscript` when stop fired). Restores the audio session to
    /// `.briefingPlayback` so the briefing audio comes back un-ducked.
    @discardableResult
    func stop() -> String {
        let final = liveTranscript
        audioEngine?.stop()
        recognitionRequest?.endAudio()
        teardown(restoreSession: true)
        if phase != .denied, phase != .failed("") {
            phase = .idle
        }
        logger.info("Briefing mic capture stopped (chars=\(final.count))")
        return final
    }

    // MARK: - Private

    private func teardown(restoreSession: Bool) {
        audioEngine?.inputNode.removeTap(onBus: 0)
        audioEngine?.stop()
        audioEngine = nil
        recognitionTask?.cancel()
        recognitionTask = nil
        recognitionRequest = nil
        if restoreSession {
            // Drop back to plain briefing playback so the duck releases and
            // the briefing audio resumes at full volume.
            try? AudioSessionCoordinator.shared.activate(.briefingPlayback)
        }
    }
}
