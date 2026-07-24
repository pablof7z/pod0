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
    let nostrSigner: SubscriptionId

    nonisolated func unsubscribeAll(from facade: Pod0Facade) {
        for id in [
            library, playback, recallConfiguration, chapterWorkflows, notes,
            memories, clips, downloads, transcriptWorkflows, notificationSettings,
            scheduledAgents, nostrSigner,
        ] {
            facade.unsubscribe(subscriptionId: id)
        }
    }
}

extension SharedLibraryClient {
    nonisolated static func makeSubscriptions(
        facade: Pod0Facade,
        subscriber: SharedLibrarySubscriber
    ) -> SharedLibrarySubscriptions {
        func subscribe(_ scope: ProjectionScope, maxItems: UInt16 = 200) -> SubscriptionId {
            facade.subscribe(
                request: ProjectionRequest(scope: scope, offset: 0, maxItems: maxItems),
                subscriber: subscriber
            )
        }
        return SharedLibrarySubscriptions(
            library: subscribe(.library),
            playback: subscribe(.playback),
            recallConfiguration: subscribe(.recallConfiguration, maxItems: 1),
            chapterWorkflows: subscribe(.chapterWorkflows(episodeId: nil)),
            notes: subscribe(.notes(scope: .all)),
            memories: subscribe(.memories(scope: .all)),
            clips: subscribe(.clips(scope: .active)),
            downloads: subscribe(.downloads(episodeId: nil)),
            transcriptWorkflows: subscribe(.transcriptWorkflows(episodeId: nil)),
            notificationSettings: subscribe(.newEpisodeNotificationSettings, maxItems: 1),
            scheduledAgents: subscribe(.scheduledAgent(taskId: nil)),
            nostrSigner: subscribe(.nostrSigner, maxItems: 20)
        )
    }

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
        nostrSignerSubscriptionID = subscriptions.nostrSigner
    }
}
