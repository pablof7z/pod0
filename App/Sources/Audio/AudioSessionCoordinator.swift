import AVFoundation
import Foundation
import os.log

// MARK: - VoiceSessionClient (protocol stub for Lane 8)

/// Hook a future `AudioConversationManager` (Lane 8) would conform to so the
/// coordinator can call back when voice mode activates / deactivates.
///
/// Lane 1 ships a default no-op so the build is green before Lane 8 lands.
protocol VoiceSessionClient: AnyObject, Sendable {
    func voiceSessionWillActivate() async
    func voiceSessionDidDeactivate() async
}

// MARK: - AudioSessionCoordinator

/// Single arbiter of `AVAudioSession` for the whole app. Three current callers
/// share a process-wide audio session: the podcast `AudioEngine`, the existing
/// `VoiceItemService` (note dictation), and the future `AudioConversationManager`
/// (voice-mode conversational agent, Lane 8). Without coordination the three
/// fight over `setCategory` / `setMode` and the mic/speaker route flickers.
///
/// All transitions go through `activate(_:)`. The coordinator records the last
/// requested mode so re-entrant calls (e.g. starting voice mode while podcast
/// playback is active) just no-op.
///
/// The recommendations here track `docs/spec/research/voice-stt-tts-stack.md`:
/// `.playback` + `.spokenAudio` for podcast-only, `.playAndRecord` + `.voiceChat`
/// + `setPrefersEchoCancelledInput(true)` (iOS 18+) for full-duplex voice.
@MainActor
final class AudioSessionCoordinator {

    // MARK: - Singleton

    static let shared = AudioSessionCoordinator()

    // MARK: - Mode

    /// What the session is currently configured for.
    ///
    /// - `.idle`: no caller has asked to play or record. Session deactivated.
    /// - `.podcastPlayback`: `.playback` + `.spokenAudio`, route ducks for Siri.
    /// - `.briefingPlayback`: same category as `.podcastPlayback`; distinct case
    ///   so callers can reason about *what* is playing without inspecting the
    ///   playback engine. Voice mode duck/resume logic keys on this.
    /// - `.voiceCapture`: `.record` + `.measurement`, used by `VoiceItemService`
    ///   for note dictation (no playback).
    /// - `.duckedForVoice`: `.playAndRecord` + `.voiceChat` with AEC. Used while
    ///   the user is conversing with the agent over an active briefing.
    enum Mode: Equatable, Sendable {
        case idle
        case podcastPlayback
        case briefingPlayback
        case voiceCapture
        case duckedForVoice
    }

    // MARK: - State

    private(set) var mode: Mode = .idle

    /// Optional voice-mode client (Lane 8). The default `NoopVoiceSessionClient`
    /// is wired at init so unit tests and the current build don't crash if
    /// voice mode isn't compiled in.
    weak var voiceClient: (any VoiceSessionClient)?

    // MARK: - Private

    private let logger = Logger.app("AudioSessionCoordinator")
    private let session = AVAudioSession.sharedInstance()

    // MARK: - Init

    private init() {}

    // MARK: - Public API

    /// Reconfigure `AVAudioSession` for the requested mode.
    ///
    /// Idempotent: calling `activate(.podcastPlayback)` while already in that
    /// mode is a no-op. Throws if `AVAudioSession` rejects the configuration.
    func activate(_ mode: Mode) throws {
        guard mode != self.mode else { return }
        logger.info("AudioSession transition: \(String(describing: self.mode)) → \(String(describing: mode))")

        switch mode {
        case .idle:
            try deactivate()
        case .podcastPlayback, .briefingPlayback:
            try configurePlayback()
        case .voiceCapture:
            try configureVoiceCapture()
        case .duckedForVoice:
            try configureDuckedForVoice()
        }
        self.mode = mode
    }

    /// Convenience: route between briefing/podcast playback without thrashing
    /// the underlying category (both use `.playback` + `.spokenAudio`).
    func switchPlaybackContext(to mode: Mode) {
        precondition(mode == .podcastPlayback || mode == .briefingPlayback,
                     "switchPlaybackContext only accepts playback modes")
        if self.mode == .podcastPlayback || self.mode == .briefingPlayback {
            // Same category — just relabel for the rest of the app.
            self.mode = mode
        } else {
            try? activate(mode)
        }
    }

    /// Tear down — used by tests, by app backgrounding-with-no-active-playback,
    /// and by callers who explicitly stop everything.
    func deactivate() throws {
        try session.setActive(false, options: .notifyOthersOnDeactivation)
        mode = .idle
    }

    // MARK: - Voice mode high-level state

    /// Coarse-grained session state used by the conversational voice layer.
    ///
    /// The fine-grained `Mode` API (above) survives because the briefing
    /// player and the podcast engine reason about it directly. `SessionState`
    /// is a higher-level facade Voice mode flips between when a conversation
    /// starts or ends — exactly the two stable shapes called for in the
    /// research note (`docs/spec/research/voice-stt-tts-stack.md` §3):
    ///
    /// - **`.playbackOnly`** → `.playback` + `.spokenAudio`. Briefings and
    ///   episodes; ducks for system spoken audio (Siri / nav).
    /// - **`.conversation`** → `.playAndRecord` + `.voiceChat` +
    ///   `setPrefersEchoCancelledInput(true)` + `.duckOthers`. Used while
    ///   the conversational agent is active. Never `.mixWithOthers` — it
    ///   would disable AEC and reintroduce speaker bleed into barge-in.
    enum SessionState: Sendable, Equatable {
        case playbackOnly
        case conversation
    }

    /// Single mutation point for conversational voice mode. Maps the high
    /// level state to the fine-grained `Mode` enum and re-routes the
    /// underlying session. Idempotent — no-op when already in the
    /// requested state.
    func setState(_ state: SessionState) throws {
        switch state {
        case .playbackOnly:
            // Prefer briefing context if we were already in conversation
            // backed by briefing playback; otherwise plain podcast.
            let target: Mode = (mode == .duckedForVoice && lastPlaybackContext == .briefingPlayback)
                ? .briefingPlayback
                : .podcastPlayback
            try activate(target)
        case .conversation:
            // Remember the playback context so we can return to it.
            if mode == .podcastPlayback || mode == .briefingPlayback {
                lastPlaybackContext = mode
            }
            try activate(.duckedForVoice)
        }
    }

    /// Tracks the last playback mode prior to entering `.duckedForVoice` so
    /// `setState(.playbackOnly)` can return to the correct context.
    private var lastPlaybackContext: Mode = .podcastPlayback

    // MARK: - Configurations

    private func configurePlayback() throws {
        try session.setCategory(.playback, mode: .spokenAudio, options: [])
        try session.setActive(true)
    }

    private func configureVoiceCapture() throws {
        // Mirrors what `VoiceItemService` sets today so a future migration is
        // a one-line change there: `try AudioSessionCoordinator.shared.activate(.voiceCapture)`.
        try session.setCategory(.record, mode: .measurement, options: .duckOthers)
        try session.setActive(true, options: .notifyOthersOnDeactivation)
    }

    private func configureDuckedForVoice() throws {
        try session.setCategory(
            .playAndRecord,
            mode: .voiceChat,
            options: [.duckOthers, .defaultToSpeaker, .allowBluetoothHFP, .allowBluetoothA2DP]
        )
        if session.isEchoCancelledInputAvailable {
            try session.setPrefersEchoCancelledInput(true)
        }
        try session.setActive(true, options: .notifyOthersOnDeactivation)
    }
}

// MARK: - Default no-op voice client

/// Default placeholder so callers never have to nil-check before notifying
/// the voice subsystem. Lane 8 swaps the real `AudioConversationManager` in
/// via `AudioSessionCoordinator.shared.voiceClient = manager`.
final class NoopVoiceSessionClient: VoiceSessionClient {
    func voiceSessionWillActivate() async {}
    func voiceSessionDidDeactivate() async {}
}
