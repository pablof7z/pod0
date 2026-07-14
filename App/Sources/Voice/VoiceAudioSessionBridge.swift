import Foundation

// MARK: - AudioSessionCoordinatorProtocol

/// Bridge between Lane 8 (Voice) and Lane 1 (Audio).
///
/// The Audio lane owns the singleton `AVAudioSession` configuration so that
/// podcast playback, briefing playback, and voice conversation never fight
/// over the route. This protocol defines the surface Voice needs from that
/// owner — Lane 1 supplies the concrete implementation at integration.
///
/// During the single-lane build phase we install a `NoopAudioSessionCoordinator`
/// so `AudioConversationManager` is constructible and behaves sensibly in
/// previews and tests.
///
/// All methods are `async` because real implementations may need to wait for
/// the session category to take effect, and `throws` because category
/// changes can fail (e.g. interrupted by another app, hardware unavailable).
///
/// Sendable: implementations live behind an actor or are themselves
/// `@MainActor`-isolated so the caller can hop appropriately.
protocol AudioSessionCoordinatorProtocol: Sendable {

    /// Acquire a record + playback configuration suited for STT capture
    /// while still allowing TTS / app audio to play through the speaker.
    /// Used at the start of a voice turn.
    func beginVoiceCapture() async throws

    /// Switch to playback-priority configuration for TTS output.
    /// Used between STT end and TTS start so the speaker is loud and the
    /// mic input is suspended (saves battery, prevents re-entry of TTS into
    /// the recogniser).
    func beginVoicePlayback() async throws

    /// Duck other audio (e.g. podcast playback) so a briefing or
    /// notification-style assistant message is intelligible without
    /// stopping background media. Lane 9 invokes this via the briefing
    /// handoff. Called by `AudioConversationManager.attachToBriefing`.
    func duckOthersForBriefing() async throws

    /// Restore normal playback / capture mix after a briefing finishes.
    func unduckOthersAfterBriefing() async throws

    /// Tear the session down completely. Called when the user exits Voice
    /// mode. The Audio lane is free to keep its session alive for podcast
    /// playback if it's still active — implementations should be defensive.
    func endVoiceSession() async
}

// MARK: - NoopAudioSessionCoordinator

/// Default no-op coordinator used in previews, tests, and during integration
/// before Lane 1 wires its real implementation. Logs only — never mutates the
/// shared `AVAudioSession`. Safe to substitute on any thread; methods are
/// trivial returns.
final class NoopAudioSessionCoordinator: AudioSessionCoordinatorProtocol {

    func beginVoiceCapture() async throws {}
    func beginVoicePlayback() async throws {}
    func duckOthersForBriefing() async throws {}
    func unduckOthersAfterBriefing() async throws {}
    func endVoiceSession() async {}
}
