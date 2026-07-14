import Foundation

// USAGE:
// Internal-only dispatcher used by transcript-segment surfaces (clip composer,
// quote share) when their own long-press chooses to escalate a single line to
// the agent. The primary chapter-row long-press now uses
// `ChapterAskAgentDispatcher` instead, which writes a `ChapterAgentContext`
// with no transcript text. Both paths share the `.askAgentRequested`
// notification and `AgentChatSession`'s drain.

// MARK: - Ask-the-agent dispatcher (transcript-segment flavour)
//
// Long-press → "Ask the agent about this" wiring.
//
// Lives in its own file because `PlayerTranscriptScrollView` sits within ten
// lines of the 300-line soft cap (see `AGENTS.md`). Implemented as a stateless
// helper rather than a true extension so we don't need to loosen the View's
// `private var store` access.
//
// Flow:
//   1. `PlayerTranscriptRow.contextMenu` calls `onAskAgent` with the tapped
//      segment.
//   2. `PlayerTranscriptScrollView.askAgent(about:)` forwards to
//      `AskAgentDispatcher.dispatch`, supplying the live `Episode` + `store`.
//   3. Dispatcher writes `AppStateStore.pendingTranscriptAgentContext` (mirrors
//      the `pendingFriendInvite` pattern in `RootView`).
//   4. Dispatcher posts `.askAgentRequested`; `RootView` flips
//      `showAgentChat = true`.
//   5. `AgentChatSession.init` drains the field once and prefills the composer.
//
// Notification-based wakeup matches `voiceModeRequested` — already proven for
// cross-stack presentations of full-screen surfaces.

extension Notification.Name {
    /// Posted by the player's transcript long-press to open the agent chat
    /// sheet. `RootView` observes and presents `AgentChatView`.
    static let askAgentRequested = Notification.Name("io.f7z.podcast.askAgentRequested")
    /// Posted when episode playback is initiated from a list row's play button.
    /// `RootView` observes and expands the full player sheet.
    static let openPlayerRequested = Notification.Name("io.f7z.podcast.openPlayerRequested")
    /// Posted by the player's clip-source chip when the user taps to view the
    /// source episode. `userInfo["episodeID"]` carries the UUID string.
    /// `RootView` dismisses the player and presents `EpisodeDetailView`.
    static let openEpisodeDetailRequested = Notification.Name("io.f7z.podcast.openEpisodeDetailRequested")
    /// Posted by the player's More menu when the user taps "Go to show".
    /// `userInfo["subscriptionID"]` carries the podcast UUID string.
    /// `RootView` dismisses the player and presents `ShowDetailView` —
    /// sibling of `openEpisodeDetailRequested`. Both bindings update in the
    /// same render tick so SwiftUI can swap one sheet for the other without
    /// the "present-while-dismissing" conflict that the old URL round-trip
    /// in `PlayerMoreMenu` was tripping.
    static let openSubscriptionDetailRequested = Notification.Name("io.f7z.podcast.openSubscriptionDetailRequested")
    /// Posted by `PlayerGenerationSourceChip` when the user taps an in-app
    /// chat source. `userInfo["conversationID"]` carries the `UUID`. `RootView`
    /// dismisses the player, switches to the target conversation, and opens
    /// the agent chat sheet.
    static let openAgentChatConversation = Notification.Name("io.f7z.podcast.openAgentChatConversation")
    /// Posted by `PlayerGenerationSourceChip` when the user taps a Nostr
    /// conversation source. `userInfo["rootEventID"]` carries the root event
    /// ID string. `RootView` dismisses the player and presents
    /// `NostrConversationDetailView`.
    static let openNostrConversationRequested = Notification.Name("io.f7z.podcast.openNostrConversationRequested")
}

enum AskAgentDispatcher {

    /// Builds the `TranscriptAgentContext` for the long-pressed segment and
    /// publishes it. Silently no-ops if the player has no current episode —
    /// long-press should never fire in that state but the guard keeps the
    /// helper honest.
    @MainActor
    static func dispatch(
        segment: Segment,
        episode: Episode?,
        store: AppStateStore
    ) {
        guard let episode else { return }
        let title = store.podcast(id: episode.podcastID)?.title ?? ""
        let context = TranscriptAgentContext(
            episodeID: episode.id,
            subscriptionTitle: title,
            segmentText: segment.text,
            timestamp: segment.start
        )
        store.pendingTranscriptAgentContext = context
        Haptics.light()
        NotificationCenter.default.post(name: .askAgentRequested, object: nil)
    }
}
