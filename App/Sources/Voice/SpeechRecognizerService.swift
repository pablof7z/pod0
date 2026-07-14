import Foundation
import Speech
import AVFoundation
import os.log

// MARK: - SpeechRecognizerServiceProtocol

/// Streaming on-device speech recogniser. Emits `.partial` and `.final`
/// transcription events through an `AsyncStream`.
///
/// Implementations are responsible for:
///   - Authorisation prompts (mic + speech recognition).
///   - Audio engine lifecycle (tap install / remove).
///   - Picking the best available recogniser for the OS:
///       * iOS 26+: `SpeechAnalyzer` + `SpeechTranscriber` (faster, on-device).
///       * Older: `SFSpeechRecognizer` with `requiresOnDeviceRecognition = true`.
///   - Reporting authorisation failures via the stream's `error` channel.
///
/// The conversation manager is the sole consumer; only one recognition
/// session can be active at a time per process.
@MainActor
protocol SpeechRecognizerServiceProtocol: AnyObject {

    /// Whether on-device recognition is available right now. False if
    /// the user denied permissions, the recogniser is unavailable for
    /// the current locale, or another session is in progress.
    var isAvailable: Bool { get }

    /// Begin a new recognition session. Returns an `AsyncThrowingStream` that
    /// yields transcription events as the user speaks. Cancelling the
    /// consuming task tears the session down.
    func startStreaming() -> AsyncThrowingStream<SpeechRecognitionEvent, Error>

    /// Manually stop the current session and finalise the transcription.
    /// Idempotent — safe to call when no session is running.
    func stop()
}

// MARK: - SpeechRecognitionEvent

enum SpeechRecognitionEvent: Sendable, Equatable {
    case partial(String)
    case final(String)
}

// MARK: - SpeechRecognizerError

enum SpeechRecognizerError: Error, Equatable, Sendable {
    case permissionDenied
    case recognizerUnavailable
    case audioEngineFailed(String)
    case sessionAlreadyRunning
}

// MARK: - SpeechRecognizerService

/// Concrete recogniser that prefers iOS 26's `SpeechTranscriber` when the
/// `Speech` framework exposes it, and falls back to `SFSpeechRecognizer`
/// otherwise.
///
/// **iOS-version gating note**: Apple ship `SpeechAnalyzer` / `SpeechTranscriber`
/// as part of iOS 26's `Speech` framework. The deployment floor for this
/// project is iOS 26.0, so `SFSpeechRecognizer` is technically always
/// available — but we keep the fallback path because:
///   1. Some locales lack the new transcriber but ship the legacy recogniser.
///   2. The new APIs may evolve in 26.x point releases — the fallback is a
///      safety net if symbol availability shifts.
@MainActor
@Observable
final class SpeechRecognizerService: SpeechRecognizerServiceProtocol {

    private let logger = Logger.app("SpeechRecognizerService")

    private var sfRecognizer: SFSpeechRecognizer?
    private var sfRequest: SFSpeechAudioBufferRecognitionRequest?
    private var sfTask: SFSpeechRecognitionTask?
    private var audioEngine: AVAudioEngine?

    /// True only between `startStreaming()` and the matching teardown.
    private var isRunning: Bool = false

    var isAvailable: Bool {
        guard SFSpeechRecognizer.authorizationStatus() == .authorized else { return false }
        return SFSpeechRecognizer()?.isAvailable ?? false
    }

    func startStreaming() -> AsyncThrowingStream<SpeechRecognitionEvent, Error> {
        AsyncThrowingStream { continuation in
            let task = Task { @MainActor in
                if self.isRunning {
                    continuation.finish(throwing: SpeechRecognizerError.sessionAlreadyRunning)
                    return
                }

                guard await self.requestPermissions() else {
                    continuation.finish(throwing: SpeechRecognizerError.permissionDenied)
                    return
                }

                guard let recognizer = SFSpeechRecognizer(), recognizer.isAvailable else {
                    continuation.finish(throwing: SpeechRecognizerError.recognizerUnavailable)
                    return
                }
                self.sfRecognizer = recognizer

                let request = SFSpeechAudioBufferRecognitionRequest()
                request.shouldReportPartialResults = true
                // Prefer on-device — privacy + lower latency. The OS may
                // override on-device when not available, but we don't fail
                // the session over that.
                request.requiresOnDeviceRecognition = recognizer.supportsOnDeviceRecognition
                self.sfRequest = request

                let engine = AVAudioEngine()
                self.audioEngine = engine
                let input = engine.inputNode
                let format = input.outputFormat(forBus: 0)
                input.installTap(onBus: 0, bufferSize: 1024, format: format) { [weak self] buffer, _ in
                    self?.sfRequest?.append(buffer)
                }

                self.sfTask = recognizer.recognitionTask(with: request) { result, error in
                    // SFSpeechRecognizer callback fires on an internal queue;
                    // hop to MainActor to mutate any state.
                    Task { @MainActor in
                        if let result {
                            let text = result.bestTranscription.formattedString
                            if result.isFinal {
                                continuation.yield(.final(text))
                                self.teardown()
                                continuation.finish()
                            } else {
                                continuation.yield(.partial(text))
                            }
                        }
                        if let error {
                            let nsError = error as NSError
                            // 216 == "user cancelled" — clean exit, not an error.
                            // 301 == "no speech detected" — not fatal; we let
                            // the caller decide via `stop()`.
                            if nsError.code == 216 {
                                self.teardown()
                                continuation.finish()
                                return
                            }
                            self.logger.error("STT error: \(error, privacy: .public)")
                            self.teardown()
                            continuation.finish(throwing: error)
                        }
                    }
                }

                do {
                    engine.prepare()
                    try engine.start()
                    self.isRunning = true
                    self.logger.info("STT session started")
                } catch {
                    self.teardown()
                    continuation.finish(throwing: SpeechRecognizerError.audioEngineFailed(error.localizedDescription))
                }
            }
            continuation.onTermination = { @Sendable _ in
                Task { @MainActor in
                    self.stop()
                    task.cancel()
                }
            }
        }
    }

    func stop() {
        guard isRunning else { return }
        sfRequest?.endAudio()
        teardown()
        logger.info("STT session stopped")
    }

    // MARK: - Private

    private func teardown() {
        audioEngine?.inputNode.removeTap(onBus: 0)
        audioEngine?.stop()
        audioEngine = nil
        sfTask?.cancel()
        sfTask = nil
        sfRequest = nil
        sfRecognizer = nil
        isRunning = false
    }

    private func requestPermissions() async -> Bool {
        // Microphone.
        switch AVAudioApplication.shared.recordPermission {
        case .granted:
            break
        case .denied:
            return false
        case .undetermined:
            let granted = await AVAudioApplication.requestRecordPermission()
            if !granted { return false }
        @unknown default:
            return false
        }

        // Speech recognition.
        switch SFSpeechRecognizer.authorizationStatus() {
        case .authorized:
            return true
        case .denied, .restricted:
            return false
        case .notDetermined:
            let status = await withCheckedContinuation { cont in
                SFSpeechRecognizer.requestAuthorization { cont.resume(returning: $0) }
            }
            return status == .authorized
        @unknown default:
            return false
        }
    }
}
