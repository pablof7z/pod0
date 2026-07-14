import Foundation

/// Transient context handed from the player's chapter long-press to the agent
/// chat surface.
///
/// Carries chapter title + start/end time only — never transcript text. The
/// agent fetches the underlying transcript window through its existing tool
/// inventory (`query_transcripts`, `search_transcripts`) using `episodeID` and
/// the time range. The user reads a chapter title, asks a question, and gets
/// an answer; they never see the transcript text.
///
/// Mirrors `TranscriptAgentContext`'s read-and-clear pattern via
/// `AppStateStore.pendingChapterAgentContext`; `AgentChatSession` drains it on
/// init and seeds the composer once.
struct ChapterAgentContext: Equatable, Identifiable {
    let id: UUID
    let episodeID: UUID
    let subscriptionTitle: String
    let episodeTitle: String
    let chapterTitle: String
    let startTime: TimeInterval
    /// Chapter end as supplied by the chapter source (`Episode.Chapter.endTime`),
    /// or the next chapter's `startTime` resolved by the dispatcher. `nil` for
    /// the final chapter when no duration is known — the agent can still scope
    /// to a half-open `[start, ∞)` window in that case.
    let endTime: TimeInterval?

    init(
        episodeID: UUID,
        subscriptionTitle: String,
        episodeTitle: String,
        chapterTitle: String,
        startTime: TimeInterval,
        endTime: TimeInterval?
    ) {
        self.id = UUID()
        self.episodeID = episodeID
        self.subscriptionTitle = subscriptionTitle
        self.episodeTitle = episodeTitle
        self.chapterTitle = chapterTitle
        self.startTime = startTime
        self.endTime = endTime
    }

    /// Composer prefill. Frames the question around the chapter title + time
    /// range. The agent has the episode handle (via tool inventory) and can
    /// pull the transcript window itself — the user never sees the raw text.
    var prefilledDraft: String {
        let show = subscriptionTitle.isEmpty ? "this episode" : subscriptionTitle
        let stamp = Self.formatStamp(startTime)
        let endLabel = endTime.map { " – \(Self.formatStamp($0))" } ?? ""
        let trimmedChapter = chapterTitle.trimmingCharacters(in: .whitespacesAndNewlines)
        let chapterLabel = trimmedChapter.isEmpty ? "this chapter" : trimmedChapter
        return "About the \"\(chapterLabel)\" chapter in \(show) (\(stamp)\(endLabel)):\n\n"
    }

    /// `mm:ss` for under an hour, `h:mm:ss` otherwise. Matches the player
    /// scrubber / chapter row formatting so the composer reads consistently
    /// with the surfaces the user just tapped.
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
