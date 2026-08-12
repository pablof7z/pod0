import Pod0Core

struct SharedLibrarySubscriptions: @unchecked Sendable {
    let library: SubscriptionId
    let playback: SubscriptionId
    let recallConfiguration: SubscriptionId
    let chapterWorkflows: SubscriptionId
    let notes: SubscriptionId
    let memories: SubscriptionId
    let clips: SubscriptionId
    let downloads: SubscriptionId
    let transcriptWorkflows: SubscriptionId
    let notificationSettings: SubscriptionId
    let scheduledAgents: SubscriptionId

    var ids: [SubscriptionId] {
        [
            library, playback, recallConfiguration, chapterWorkflows, notes,
            memories, clips, downloads, transcriptWorkflows, notificationSettings,
            scheduledAgents,
        ]
    }
}

extension SharedLibraryClient {
    func install(_ subscriptions: SharedLibrarySubscriptions) {
        librarySubscriptionID = subscriptions.library
        playbackSubscriptionID = subscriptions.playback
        recallConfigurationSubscriptionID = subscriptions.recallConfiguration
        chapterWorkflowSubscriptionID = subscriptions.chapterWorkflows
        notesSubscriptionID = subscriptions.notes
        memoriesSubscriptionID = subscriptions.memories
        clipsSubscriptionID = subscriptions.clips
        downloadsSubscriptionID = subscriptions.downloads
        transcriptWorkflowSubscriptionID = subscriptions.transcriptWorkflows
        newEpisodeNotificationSettingsSubscriptionID = subscriptions.notificationSettings
        scheduledAgentSubscriptionID = subscriptions.scheduledAgents
    }
}
