import Foundation

// MARK: - Player-surface notifications
//
// Cross-stack presentation wakeups posted by player surfaces and observed
// by `RootView`, which owns the top-level sheet/navigation state. Notif-
// ication-based wakeup matches `voiceModeRequested` — proven for full-
// screen presentations that need to reach across the view hierarchy
// without threading a callback through every intermediate view.

extension Notification.Name {
    /// Posted by the player's transcript/chapter long-press to open the
    /// agent chat sheet. `RootView` observes and presents `AgentChatView`.
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
}
