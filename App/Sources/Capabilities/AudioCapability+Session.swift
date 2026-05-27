import AVFoundation
import Foundation

// MARK: - AVAudioSession setup for the audio executor
//
// Single arbiter for `AVAudioSession` in the new `ios/Podcast` shell.
// The legacy app's `AudioSessionCoordinator` is intentionally not
// referenced here — that singleton handles voice/briefing arbitration
// that lives outside the M3 scope. When the voice/briefing capabilities
// migrate, the two coordinators reconcile.
//
// D7: configuration is a one-time side effect, not a policy decision.
// The capability sets `.playback + .spokenAudio` because that's what an
// audio-playback executor needs; *when* to play and *what* to play stay
// with the Rust player actor.

@MainActor
extension AudioCapability {

    /// Idempotent. Configures `.playback` + `.spokenAudio` and activates
    /// the session. Called lazily on the first `Load` / `Play` so the
    /// app doesn't preempt other audio at launch — matches the legacy
    /// engine's behaviour and is the recommendation in
    /// `docs/spec/research/voice-stt-tts-stack.md`.
    func configureAudioSessionIfNeeded() {
        guard !sessionConfigured else { return }
        let session = AVAudioSession.sharedInstance()
        do {
            try session.setCategory(.playback, mode: .spokenAudio, options: [])
            try session.setActive(true)
            sessionConfigured = true
        } catch {
            // D6: a session-config failure is data, not a crash. The
            // first `Play` will still emit a `Playing` report when the
            // player actually starts; if the OS refuses to route audio
            // the user hears silence but the kernel sees consistent
            // state. A future report variant can surface the route
            // error explicitly.
            sessionConfigured = false
        }
    }
}
