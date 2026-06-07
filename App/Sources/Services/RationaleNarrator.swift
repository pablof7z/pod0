import AVFoundation
import Foundation
import Observation
import os.log

// MARK: - RationaleNarrator
//
// Reads an agent pick's rationale aloud — ElevenLabs when the user has a key
// configured, `AVSpeechSynthesizer` otherwise. While narration plays:
//
//   • The active podcast is paused (and resumed when narration ends or is
//     cancelled). We don't use `.duckOthers` because the podcast engine is
//     in-process and pausing yields a cleaner audio experience than fighting
//     `AVAudioSession` priorities mid-playback.
//   • The driving pick's `episodeID` is exposed as `narratingPickID` so the
//     view layer can pulse the speaker glyph on the right card.
//
// Lifecycle: a single shared narrator owns at most one playback at a time.
// Calling `speak(_:)` while another narration is in flight cancels the
// previous one first.

@MainActor
@Observable
final class RationaleNarrator {

    static let shared = RationaleNarrator()

    private static let logger = Logger.app("RationaleNarrator")

    // MARK: - Public state

    /// `episodeID` of the pick currently being narrated, or `nil` when idle.
    /// View layer keys the pulse animation off this value.
    private(set) var narratingPickID: UUID?

    // MARK: - Internal state

    private var audioPlayer: AVAudioPlayer?
    private var fallback: AVSpeechFallback?
    /// Whether we paused podcast playback to make room for narration. We
    /// only resume if we were the one to pause it — so a user-initiated
    /// pause during narration isn't unexpectedly undone.
    private var pausedPlaybackForNarration: Bool = false
    private var playerDelegate: AudioPlayerDelegate?
    private var playback: PlaybackState?
    private let elevenLabsTTS = ElevenLabsTTSBackendClient()

    private init() {}

    // MARK: - Public API

    /// Attach the running `PlaybackState` so the narrator can pause/resume
    /// podcast audio across a narration. Idempotent — the view layer calls
    /// this on every appearance.
    func attach(playback: PlaybackState) {
        self.playback = playback
    }

    /// Start narrating `text` for the given pick. Cancels any prior
    /// narration and pauses the podcast engine if it's currently playing.
    /// Resolves once playback *starts* — the narrator independently tracks
    /// when the audio ends and resumes the podcast then.
    func speak(pickID: UUID, text: String, voiceID: String, ttsModel: String) async {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }

        // Tap-again-to-stop: if the same pick is already narrating, treat
        // this call as a stop.
        if narratingPickID == pickID {
            stop()
            return
        }

        stop() // cancel any prior narration
        narratingPickID = pickID
        capturePlaybackPauseIfNeeded()

        if !voiceID.isEmpty {
            do {
                try await speakViaElevenLabs(text: trimmed, voiceID: voiceID, ttsModel: ttsModel)
                return
            } catch {
                Self.logger.notice("ElevenLabs rationale narration failed; falling back to AVSpeech: \(error.localizedDescription, privacy: .public)")
            }
        }

        speakViaAVSpeech(text: trimmed)
    }

    /// Stop any in-flight narration immediately and resume the podcast if
    /// we paused it. Idempotent.
    func stop() {
        audioPlayer?.stop()
        audioPlayer = nil
        playerDelegate = nil
        fallback?.stopSpeaking()
        fallback = nil
        if narratingPickID != nil {
            narratingPickID = nil
            restorePlaybackIfPausedByUs()
        }
    }

    // MARK: - Playback ducking

    private func capturePlaybackPauseIfNeeded() {
        guard let playback, playback.isPlaying else { return }
        playback.pause()
        pausedPlaybackForNarration = true
    }

    private func restorePlaybackIfPausedByUs() {
        guard pausedPlaybackForNarration else { return }
        pausedPlaybackForNarration = false
        playback?.play()
    }

    // MARK: - ElevenLabs path

    private func speakViaElevenLabs(
        text: String,
        voiceID: String,
        ttsModel: String
    ) async throws {
        let effectiveModel = ttsModel.isBlank ? "eleven_turbo_v2_5" : ttsModel.trimmed
        let audio = try await elevenLabsTTS.synthesize(
            text: text,
            voiceID: voiceID,
            model: effectiveModel
        )

        configureElevenLabsAudioPlaybackSession()
        let player = try AVAudioPlayer(data: audio.data, fileTypeHint: "mp3")
        let delegate = AudioPlayerDelegate { [weak self] in
            Task { @MainActor in self?.onPlaybackEnded() }
        }
        player.delegate = delegate
        player.prepareToPlay()
        audioPlayer = player
        playerDelegate = delegate
        player.play()
    }

    // MARK: - AVSpeech fallback path

    private func speakViaAVSpeech(text: String) {
        let f = AVSpeechFallback()
        fallback = f
        // Drain the stream just so we can finish-async when the synthesizer
        // returns control. AVSpeech plays directly through the audio path;
        // we only need to observe completion.
        Task { @MainActor [weak self] in
            do {
                for try await _ in f.synthesizeStream(text: text, voiceID: "") {}
            } catch {
                Self.logger.notice("AVSpeech fallback ended with error: \(error.localizedDescription, privacy: .public)")
            }
            self?.onPlaybackEnded()
        }
    }

    private func onPlaybackEnded() {
        narratingPickID = nil
        audioPlayer = nil
        playerDelegate = nil
        fallback = nil
        restorePlaybackIfPausedByUs()
    }
}

// MARK: - AudioPlayerDelegate

/// Thin AVAudioPlayer delegate forwarder. Lets the narrator stay
/// observable/MainActor while still hooking the completion callback.
private final class AudioPlayerDelegate: NSObject, AVAudioPlayerDelegate, @unchecked Sendable {
    let onFinish: () -> Void
    init(onFinish: @escaping () -> Void) {
        self.onFinish = onFinish
    }
    func audioPlayerDidFinishPlaying(_ player: AVAudioPlayer, successfully flag: Bool) {
        onFinish()
    }
    func audioPlayerDecodeErrorDidOccur(_ player: AVAudioPlayer, error: Error?) {
        onFinish()
    }
}
