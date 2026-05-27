import AVFoundation
import Foundation

// MARK: - Barge-in
//
// When a partial transcript arrives while the synthesizer is speaking,
// stop the active utterance immediately so the user's voice "wins" the
// turn (the classic voice-assistant interrupt behaviour).
//
// **D7 note:** in the canonical NMP architecture, this policy is a Rust
// decision (kernel watches the voiced-segment stream and emits
// `VoiceCommand::Stop`). The capability scaffold for feature #42 ships
// the policy on the iOS side temporarily because the matching Rust
// `podcast-voice::manager` is still a stub; once that lands, the body
// of `notifyPartialForBargeIn(text:)` becomes a no-op and Rust drives
// the cancellation via the existing `Stop` command path.

extension VoiceCapability {
    /// Hook called from the recognition handler on every partial result.
    /// While the synthesizer is actively speaking, a non-empty partial
    /// is treated as a barge-in: cancel the in-flight utterance and
    /// let the delegate's `didCancel` report `Stopped` back to Rust.
    func notifyPartialForBargeIn(text: String) {
        guard synthesizer.isSpeaking, !text.trimmingCharacters(in: .whitespaces).isEmpty else {
            return
        }
        synthesizer.stopSpeaking(at: .immediate)
    }
}
