import Foundation
import AVFoundation
import os.log

// MARK: - AVSpeechFallback

/// Local TTS fallback using `AVSpeechSynthesizer`.
///
/// Used when:
///   - The user has not connected an ElevenLabs API key (BYOK declined).
///   - Network is unavailable (`URLSession` errors during the WS handshake).
///   - Cost-sensitive contexts: short status announcements where premium TTS
///     is overkill.
///
/// We expose a stream of `Data` to match `TTSClientProtocol`'s shape, but
/// `AVSpeechSynthesizer` plays directly through the system audio path —
/// it doesn't hand us PCM frames. So this implementation plays the audio
/// internally and yields a sentinel `Data()` once playback finishes. The
/// manager treats the stream as "speaking until it ends" rather than
/// piping the bytes to its own player.
///
/// This is a deliberate trade-off: matching the protocol shape keeps the
/// manager simple, at the cost of `AVSpeechFallback` owning its own audio
/// session ducking. The audio coordinator still owns category configuration;
/// AVSpeech just plays into whatever route is current.
final class AVSpeechFallback: NSObject, TTSClientProtocol, @unchecked Sendable {

    private let logger = Logger.app("AVSpeechFallback")
    private let synthesizer = AVSpeechSynthesizer()

    /// Tracks the currently-active continuation so the delegate callbacks
    /// can finish the stream when speech completes or is cancelled.
    private var activeContinuation: AsyncThrowingStream<Data, Error>.Continuation?

    /// Always configured — AVSpeech is always available locally.
    var isConfigured: Bool { true }

    override init() {
        super.init()
        synthesizer.delegate = self
    }

    func synthesizeStream(text: String, voiceID: String) -> AsyncThrowingStream<Data, Error> {
        AsyncThrowingStream { continuation in
            self.activeContinuation = continuation

            let utterance = AVSpeechUtterance(string: text)
            // Pick the best matching voice for the device locale; fall back
            // to system default if none. We honour `voiceID` only if it
            // looks like a BCP-47 language tag (e.g. "en-US").
            if voiceID.contains("-"), let voice = AVSpeechSynthesisVoice(language: voiceID) {
                utterance.voice = voice
            } else if let voice = AVSpeechSynthesisVoice(language: AVSpeechSynthesisVoice.currentLanguageCode()) {
                utterance.voice = voice
            }
            utterance.rate = AVSpeechUtteranceDefaultSpeechRate
            utterance.pitchMultiplier = 1.0
            utterance.volume = 1.0

            self.synthesizer.speak(utterance)
            self.logger.info("AVSpeechFallback started")

            continuation.onTermination = { @Sendable _ in
                Task { @MainActor in
                    self.stopSpeaking()
                }
            }
        }
    }

    /// Manually halt speech. Idempotent.
    @MainActor
    func stopSpeaking() {
        if synthesizer.isSpeaking {
            synthesizer.stopSpeaking(at: .immediate)
        }
        finishContinuation(withError: nil)
    }

    private func finishContinuation(withError error: Error?) {
        guard let cont = activeContinuation else { return }
        activeContinuation = nil
        if let error {
            cont.finish(throwing: error)
        } else {
            // Yield a single empty marker so consumers waiting on the first
            // element know speech actually started, then close the stream.
            cont.yield(Data())
            cont.finish()
        }
    }
}

// MARK: - AVSpeechSynthesizerDelegate

extension AVSpeechFallback: AVSpeechSynthesizerDelegate {

    func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didFinish utterance: AVSpeechUtterance) {
        finishContinuation(withError: nil)
    }

    func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didCancel utterance: AVSpeechUtterance) {
        finishContinuation(withError: nil)
    }
}
