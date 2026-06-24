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
///
/// Interruption & route-change handling:
/// iOS can deactivate the audio session externally (phone call, Siri, media from
/// another app). When that happens the OS posts
/// `AVAudioSession.interruptionNotification`. The coordinator:
///   1. On `.began` — resets `mode` to `.idle` so the next `activate()` call
///      actually runs `setActive(true)` instead of no-opping on the stale mode.
///   2. On `.ended` with `.shouldResume` — fires `onInterruptionEnd` so the
///      engine can call `play()` and restore audio without user interaction.
///
/// When headphones are unplugged iOS pauses AVPlayer automatically, but the
/// engine's state stays `.playing`, so the Now-Playing controls show a stale
/// play-button. The coordinator fires `onRouteChangeOutputLost` so the engine
/// can sync its state to `.paused`.
@MainActor
final class AudioSessionCoordinator {

    // MARK: - Singleton

    static let shared = AudioSessionCoordinator()

    // MARK: - Mode

    /// What the session is currently configured for.
    ///
    /// - `.idle`: no caller has asked to play or record. Session deactivated.
    /// - `.podcastPlayback`: `.playback` + `.spokenAudio`, route ducks for Siri.
    /// - `.voiceCapture`: `.record` + `.measurement`, used by `VoiceItemService`
    ///   for note dictation (no playback).
    /// - `.duckedForVoice`: `.playAndRecord` + `.voiceChat` with AEC. Used while
    ///   the user is conversing with the agent over active playback.
    enum Mode: Equatable, Sendable {
        case idle
        case podcastPlayback
        case voiceCapture
        case duckedForVoice
    }

    // MARK: - State

    private(set) var mode: Mode = .idle

    /// Optional voice-mode client (Lane 8). The default `NoopVoiceSessionClient`
    /// is wired at init so unit tests and the current build don't crash if
    /// voice mode isn't compiled in.
    weak var voiceClient: (any VoiceSessionClient)?

    // MARK: - Interruption / route-change callbacks

    /// Called on the `MainActor` when an audio-session interruption begins.
    /// The engine uses this to record whether it was playing so it can decide
    /// whether to auto-resume when the interruption ends.
    var onInterruptionBegan: (() -> Void)?

    /// Called on the `MainActor` when an audio-session interruption ends and
    /// `AVAudioSession.InterruptionOptions.shouldResume` is set. The engine
    /// should call `play()` from this closure to restore background audio
    /// without requiring a user tap.
    var onInterruptionEnd: (() -> Void)?

    /// Called on the `MainActor` when the output route changes in a way that
    /// indicates the previous output (e.g. headphones) was disconnected. The
    /// engine should pause playback so the Now-Playing controls reflect the
    /// real player state — iOS already silences AVPlayer, but the in-process
    /// `AudioEngine.state` stays `.playing` until told otherwise.
    var onRouteChangeOutputLost: (() -> Void)?

    // MARK: - Private

    private let logger = Logger.app("AudioSessionCoordinator")
    private let session = AVAudioSession.sharedInstance()

    // Retained notification token. Stored as `Any?` so `NotificationCenter`
    // keeps a strong reference; nil means the observer was not yet installed.
    private var interruptionObserver: NSObjectProtocol?
    private var routeChangeObserver: NSObjectProtocol?

    // MARK: - Init

    private init() {
        installNotificationObservers()
    }

    // MARK: - Public API

    /// Reconfigure `AVAudioSession` for the requested mode.
    ///
    /// Idempotent for the same mode UNLESS `self.mode` was externally reset to
    /// `.idle` by the interruption handler, which forces a real `setActive(true)`
    /// call on the next `activate(.podcastPlayback)` invocation — required to
    /// restore background audio after a phone call or Siri interruption.
    ///
    /// Throws if `AVAudioSession` rejects the configuration.
    func activate(_ mode: Mode) throws {
        guard mode != self.mode else { return }
        logger.info("AudioSession transition: \(String(describing: self.mode)) → \(String(describing: mode))")

        switch mode {
        case .idle:
            try deactivate()
        case .podcastPlayback:
            try configurePlayback()
        case .voiceCapture:
            try configureVoiceCapture()
        case .duckedForVoice:
            try configureDuckedForVoice()
        }
        self.mode = mode
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
    /// The fine-grained `Mode` API (above) survives because the podcast
    /// engine reasons about it directly. `SessionState`
    /// is a higher-level facade Voice mode flips between when a conversation
    /// starts or ends — exactly the two stable shapes called for in the
    /// research note (`docs/spec/research/voice-stt-tts-stack.md` §3):
    ///
    /// - **`.playbackOnly`** → `.playback` + `.spokenAudio`. Episodes; ducks
    ///   for system spoken audio (Siri / nav).
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
            try activate(.podcastPlayback)
        case .conversation:
            try activate(.duckedForVoice)
        }
    }

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

    // MARK: - Notification observers

    private func installNotificationObservers() {
        interruptionObserver = NotificationCenter.default.addObserver(
            forName: AVAudioSession.interruptionNotification,
            object: session,
            queue: .main
        ) { [weak self] note in
            MainActor.assumeIsolated { self?.handleInterruption(note) }
        }
        routeChangeObserver = NotificationCenter.default.addObserver(
            forName: AVAudioSession.routeChangeNotification,
            object: session,
            queue: .main
        ) { [weak self] note in
            MainActor.assumeIsolated { self?.handleRouteChange(note) }
        }
    }

    // MARK: - Interruption handling

    private func handleInterruption(_ note: Notification) {
        guard
            let info = note.userInfo,
            let typeValue = info[AVAudioSessionInterruptionTypeKey] as? UInt,
            let type = AVAudioSession.InterruptionType(rawValue: typeValue)
        else { return }

        switch type {
        case .began:
            // Notify the engine BEFORE resetting mode so it can capture
            // whether it was playing at this moment (state will transition
            // to `.paused` via AVPlayer KVO shortly after, so we record
            // intent here while it's still accurate).
            onInterruptionBegan?()
            // iOS has deactivated the session. Reset our mode tracking so the
            // next `activate(_:)` call skips the idempotency guard and actually
            // calls `setActive(true)`. Without this reset the guard
            // `guard mode != self.mode` no-ops and the session is never reactivated.
            logger.info("AudioSession interruption began — mode reset to .idle")
            mode = .idle

        case .ended:
            guard
                let optionsValue = info[AVAudioSessionInterruptionOptionKey] as? UInt
            else { return }
            let options = AVAudioSession.InterruptionOptions(rawValue: optionsValue)
            if options.contains(.shouldResume) {
                logger.info("AudioSession interruption ended with shouldResume — firing callback")
                onInterruptionEnd?()
            } else {
                logger.info("AudioSession interruption ended without shouldResume")
            }

        @unknown default:
            break
        }
    }

    // MARK: - Route change handling

    private func handleRouteChange(_ note: Notification) {
        guard
            let info = note.userInfo,
            let reasonValue = info[AVAudioSessionRouteChangeReasonKey] as? UInt,
            let reason = AVAudioSession.RouteChangeReason(rawValue: reasonValue)
        else { return }

        switch reason {
        case .oldDeviceUnavailable:
            // Headphones / AirPods disconnected. iOS silences AVPlayer
            // automatically, but the engine's state stays `.playing` until
            // we tell it otherwise — so the lock-screen controls show a
            // stale "playing" state. Notify the engine to sync to `.paused`.
            logger.info("AudioSession route change: output lost — firing pause callback")
            onRouteChangeOutputLost?()

        default:
            break
        }
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
