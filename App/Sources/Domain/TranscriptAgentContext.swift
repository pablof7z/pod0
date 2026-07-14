import Foundation

/// Transient context handed from the player's transcript long-press to the
/// agent chat surface.
///
/// The flow mirrors `pendingFriendInvite` (see `AppStateStore.swift`): the
/// long-press handler sets `AppStateStore.pendingTranscriptAgentContext`,
/// `RootView` opens the agent chat sheet, and `AgentChatSession` drains the
/// field once on creation — so the seed lands in the composer exactly once,
/// never replayed on a later sheet re-presentation.
struct TranscriptAgentContext: Equatable, Identifiable {
    let id: UUID
    let episodeID: UUID
    let subscriptionTitle: String
    let segmentText: String
    let timestamp: TimeInterval

    init(
        episodeID: UUID,
        subscriptionTitle: String,
        segmentText: String,
        timestamp: TimeInterval
    ) {
        self.id = UUID()
        self.episodeID = episodeID
        self.subscriptionTitle = subscriptionTitle
        self.segmentText = segmentText
        self.timestamp = timestamp
    }

    /// Composer prefill. Markdown blockquote keeps the original line visually
    /// distinct from whatever the user appends. Show name + timestamp give the
    /// agent the grounding it needs to call `query_transcripts` / `play_episode`
    /// without a follow-up clarification.
    var prefilledDraft: String {
        let mins = Int(timestamp) / 60
        let secs = Int(timestamp) % 60
        let stamp = String(format: "%d:%02d", mins, secs)
        let trimmed = segmentText.trimmingCharacters(in: .whitespacesAndNewlines)
        let show = subscriptionTitle.isEmpty ? "this episode" : subscriptionTitle
        return "About this moment in \(show) at \(stamp):\n\n> \(trimmed)\n\n"
    }
}
