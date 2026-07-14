import Foundation

/// Transient context written by `VoiceNoteRecordingSheet` after the user
/// records a voice note in the player. Mirrors `ChapterAgentContext`'s
/// read-and-clear pattern via `AppStateStore.pendingVoiceNoteAgentContext`.
///
/// Carries the timestamp anchor, the active chapter bounds (if any), and
/// the user's transcribed utterance. The agent receives this as a prefilled
/// message and is auto-sent — no extra tap required.
struct VoiceNoteAgentContext: Equatable, Identifiable {
    let id: UUID
    let episodeID: UUID
    let subscriptionTitle: String
    let episodeTitle: String
    /// Playback position captured the moment the user tapped the mic button.
    let timestamp: TimeInterval
    let activeChapterTitle: String?
    let chapterStartTime: TimeInterval?
    let chapterEndTime: TimeInterval?
    /// Transcribed speech from the user's voice recording.
    let userUtterance: String

    init(
        episodeID: UUID,
        subscriptionTitle: String,
        episodeTitle: String,
        timestamp: TimeInterval,
        activeChapterTitle: String?,
        chapterStartTime: TimeInterval?,
        chapterEndTime: TimeInterval?,
        userUtterance: String
    ) {
        self.id = UUID()
        self.episodeID = episodeID
        self.subscriptionTitle = subscriptionTitle
        self.episodeTitle = episodeTitle
        self.timestamp = timestamp
        self.activeChapterTitle = activeChapterTitle
        self.chapterStartTime = chapterStartTime
        self.chapterEndTime = chapterEndTime
        self.userUtterance = userUtterance
    }

    /// Prefilled message sent automatically to the agent on sheet dismiss.
    ///
    /// Format:
    ///   At [timestamp] in "[episode]" ([show])[, during "[chapter]" ([start]–[end])]:
    ///
    ///   [user utterance]
    var prefilledDraft: String {
        let stamp = Self.formatStamp(timestamp)
        var lines: [String] = []

        var header = "At \(stamp) in \"\(episodeTitle)\""
        if !subscriptionTitle.isEmpty { header += " (\(subscriptionTitle))" }
        if let chapter = activeChapterTitle?.trimmingCharacters(in: .whitespacesAndNewlines),
           !chapter.isEmpty {
            let start = chapterStartTime.map { Self.formatStamp($0) }
            let end = chapterEndTime.map { Self.formatStamp($0) }
            var chapterLine = ", during \"\(chapter)\""
            if let s = start {
                chapterLine += " (\(s)"
                if let e = end { chapterLine += "–\(e)" }
                chapterLine += ")"
            }
            header += chapterLine
        }
        lines.append("\(header):")
        lines.append("")
        lines.append(userUtterance.trimmingCharacters(in: .whitespacesAndNewlines))
        return lines.joined(separator: "\n")
    }

    static func formatStamp(_ t: TimeInterval) -> String {
        guard t.isFinite, t >= 0 else { return "0:00" }
        let total = Int(t)
        let h = total / 3600
        let m = (total % 3600) / 60
        let s = total % 60
        return h > 0
            ? String(format: "%d:%02d:%02d", h, m, s)
            : String(format: "%d:%02d", m, s)
    }
}
