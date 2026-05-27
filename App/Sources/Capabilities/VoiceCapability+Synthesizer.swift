import AVFoundation
import Foundation

// MARK: - AVSpeechSynthesizerDelegate adapter
//
// Pulled into its own NSObject so the @MainActor capability doesn't
// inherit Objective-C delegate quirks. The delegate hops back onto the
// main actor before touching capability state. Lives in this sibling
// file to keep `VoiceCapability.swift` under the 300-LOC soft limit
// (AGENTS.md).

final class SpeechSynthesizerDelegate: NSObject, AVSpeechSynthesizerDelegate, @unchecked Sendable {
    weak var owner: VoiceCapability?

    init(owner: VoiceCapability) {
        self.owner = owner
    }

    func speechSynthesizer(
        _ synthesizer: AVSpeechSynthesizer,
        didStart utterance: AVSpeechUtterance
    ) {
        Task { @MainActor [weak owner] in
            guard let owner, let id = owner.activeSpeakRequestID else { return }
            owner.emit(.started(requestID: id))
        }
    }

    func speechSynthesizer(
        _ synthesizer: AVSpeechSynthesizer,
        didFinish utterance: AVSpeechUtterance
    ) {
        Task { @MainActor [weak owner] in
            guard let owner, let id = owner.activeSpeakRequestID else { return }
            owner.activeSpeakRequestID = nil
            owner.emit(.finished(requestID: id))
        }
    }

    func speechSynthesizer(
        _ synthesizer: AVSpeechSynthesizer,
        didCancel utterance: AVSpeechUtterance
    ) {
        Task { @MainActor [weak owner] in
            guard let owner else { return }
            owner.activeSpeakRequestID = nil
            owner.emit(.stopped)
        }
    }
}
