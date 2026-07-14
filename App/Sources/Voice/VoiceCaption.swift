import Foundation
import SwiftUI

// MARK: - VoiceCaption

/// A single caption line shown during a voice turn.
///
/// Captions are an accessibility requirement (`ux-06-voice-mode.md`): users
/// who cannot hear or who are in a quiet environment must be able to read
/// what the agent said and what the recogniser believes the user said.
///
/// We carry both `partial` and `final` states because the speech recogniser
/// streams partial hypotheses long before it commits a final transcription —
/// the UI dims partial captions to signal "this may still change."
struct VoiceCaption: Identifiable, Equatable, Sendable {

    /// Who produced this caption.
    enum Speaker: String, Sendable, Equatable {
        case user
        case agent
    }

    /// Whether the recogniser / TTS stream has committed to this text.
    enum Stability: String, Sendable, Equatable {
        case partial
        case final
    }

    let id: UUID
    let speaker: Speaker
    var text: String
    var stability: Stability
    let createdAt: Date

    init(
        id: UUID = UUID(),
        speaker: Speaker,
        text: String,
        stability: Stability = .partial,
        createdAt: Date = .init()
    ) {
        self.id = id
        self.speaker = speaker
        self.text = text
        self.stability = stability
        self.createdAt = createdAt
    }
}

// MARK: - VoiceCaptionLog

/// Append-only log of captions for the current voice session.
///
/// View-model owned by `AudioConversationManager`. Views render the most
/// recent N entries; older entries fall off the visible region but stay in
/// memory for accessibility scroll-back within a session.
@Observable
@MainActor
final class VoiceCaptionLog {

    /// Maximum number of captions retained. Older entries are evicted when
    /// the log grows beyond this — voice sessions can be long-running so we
    /// must bound memory.
    static let maxEntries: Int = 200

    private(set) var entries: [VoiceCaption] = []

    /// Most-recent caption, or `nil` if none yet.
    var latest: VoiceCaption? { entries.last }

    /// Most-recent caption from a given speaker.
    func latest(from speaker: VoiceCaption.Speaker) -> VoiceCaption? {
        entries.last { $0.speaker == speaker }
    }

    func clear() {
        entries.removeAll()
    }

    /// Appends a fresh partial caption; returns its `id` so subsequent
    /// updates from the same recogniser turn can target it via `update`.
    @discardableResult
    func appendPartial(_ speaker: VoiceCaption.Speaker, text: String) -> UUID {
        let caption = VoiceCaption(speaker: speaker, text: text, stability: .partial)
        entries.append(caption)
        trimIfNeeded()
        return caption.id
    }

    /// Replaces the text of an existing caption. Used when a partial
    /// transcription updates with new hypothesis text.
    func update(id: UUID, text: String, stability: VoiceCaption.Stability? = nil) {
        guard let idx = entries.firstIndex(where: { $0.id == id }) else { return }
        entries[idx].text = text
        if let stability { entries[idx].stability = stability }
    }

    /// Marks a caption as `.final`.
    func finalize(id: UUID, text: String? = nil) {
        guard let idx = entries.firstIndex(where: { $0.id == id }) else { return }
        if let text { entries[idx].text = text }
        entries[idx].stability = .final
    }

    /// Appends a one-shot final caption (used for assistant TTS chunks
    /// where we don't need to update the row in place).
    func appendFinal(_ speaker: VoiceCaption.Speaker, text: String) {
        let caption = VoiceCaption(speaker: speaker, text: text, stability: .final)
        entries.append(caption)
        trimIfNeeded()
    }

    private func trimIfNeeded() {
        let overflow = entries.count - Self.maxEntries
        guard overflow > 0 else { return }
        entries.removeFirst(overflow)
    }
}
