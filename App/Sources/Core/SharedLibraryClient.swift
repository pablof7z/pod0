import Foundation
import Pod0Core
@MainActor
final class SharedLibraryClient {
    static let maximumActiveChapterProjections = 8
    struct Waiter {
        let continuation: CheckedContinuation<OperationResult?, Error>
    }
    nonisolated let facade: Pod0Facade
    let commandExecutor = CoreFacadeCommandExecutor()
    let authoritativeTranscriptReader: SharedTranscriptReader
    let authoritativeChapterReader: SharedChapterReader
    let dispatcher: Pod0NativeHostDispatcher
    let agentStreamingState = CoreAgentStreamingState()
    let deferredPlaybackHost: DeferredPlaybackHost
    let deferredAgentHost: DeferredAgentHost
    let deferredRecallHost: DeferredRecallHost
    private var subscriber: SharedLibrarySubscriber?
    var librarySubscriptionID: SubscriptionId?
    var playbackSubscriptionID: SubscriptionId?
    var chapterWorkflowSubscriptionID: SubscriptionId?
    var recallConfigurationSubscriptionID: SubscriptionId?
    var notesSubscriptionID: SubscriptionId?
    var memoriesSubscriptionID: SubscriptionId?
    var clipsSubscriptionID: SubscriptionId?
    var downloadsSubscriptionID: SubscriptionId?
    var transcriptWorkflowSubscriptionID: SubscriptionId?
    var newEpisodeNotificationSettingsSubscriptionID: SubscriptionId?
    var nostrSignerSubscriptionID: SubscriptionId?
    var scheduledAgentSubscriptionID: SubscriptionId?
    var waiters: [CommandId: Waiter] = [:]
    var lastLibraryRevision: UInt64 = 0
    var lastPlaybackRevision: UInt64 = 0
    var lastChapterWorkflowRevision: UInt64 = 0
    var lastNotesRevision: UInt64 = 0
    var lastMemoriesRevision: UInt64 = 0
    var lastClipsRevision: UInt64 = 0
    weak var store: AppStateStore?
    weak var playbackState: PlaybackState?
    var cachedSnapshot: SharedLibrarySnapshot?
    var chapterScopes = ChapterProjectionScopes(
        capacity: SharedLibraryClient.maximumActiveChapterProjections
    )
    var chapterSnapshots: [UUID: SharedChapterSnapshot] = [:]
    var chapterProjectionTasks: [UUID: Task<Void, Never>] = [:]
    var announcedPublisherChapterEpisodeIDs: Set<UUID> = []
    var announcedModelChapterVersions: [UUID: String] = [:]
    var cachedPublisherChapterWorkflows: [PublisherChapterWorkflowProjection] = []
    var playbackChapterEpisodeID: UUID?
    var cachedPlayback: PlaybackProjection?
    var cachedPlaybackRevision: UInt64 = 0
    var cachedNotes: SharedNoteSnapshot?
    var cachedMemories: SharedMemorySnapshot?
    var cachedClips: SharedClipSnapshot?
    var cachedRecallConfiguration: RecallConfiguration?
    var lastDownloadsRevision: UInt64 = 0
    var cachedDownloadWorkflows: [UUID: DownloadWorkflowProjection] = [:]
    var lastTranscriptWorkflowRevision: UInt64 = 0
    var lastScheduledAgentRevision: UInt64 = 0
    var lastNostrSignerRevision: UInt64 = 0
    var cachedNostrSigner: SignerProjection?
    var cachedScheduledAgent: ScheduledAgentProjection?
    var cachedNewEpisodeNotificationSettings: NewEpisodeNotificationSettingsProjection?
    var announcedTranscriptWorkflowVersions: [UUID: String] = [:]
    var playbackHostAttached = false
    var coreCommandTail: Task<Void, Never>?
    var coreCommandGeneration: UInt64 = 0
    var subscriptionTask: Task<Void, Never>?
    var initialProjectionTask: Task<Void, Never>?
    var libraryProjectionTask: Task<Void, Never>?
    var noteProjectionTask: Task<Void, Never>?
    var memoryProjectionTask: Task<Void, Never>?
    var clipProjectionTask: Task<Void, Never>?
    var scheduledAgentProjectionTask: Task<Void, Never>?
    var downloadProjectionTask: Task<Void, Never>?
    var evidenceRebuildTask: Task<Void, Never>?
    var evidenceUpdateTasks: [UUID: Task<Void, Never>] = [:]
    var recallWaiters: [RecallQueryId: SharedRecallWaiter] = [:]
    var rebuildingEvidenceEpisodeIDs: Set<UUID> = []
    var recallHostAttached = false
    weak var workflowClient: WorkflowClient?
    let coreStoreURL: URL
    let downloadNativeStore = CoreDownloadNativeStore()

    init(
        facade: Pod0Facade,
        coreStoreURL: URL,
        feedHost: any CoreFeedHosting,
        downloadHost: any CoreDownloadHosting = UnavailableCoreDownloadHost(),
        notificationHost: any CoreNotificationHosting = CoreNotificationHost(),
        observationOutbox: NativeHostObservationOutbox? = nil
    ) {
        self.facade = facade
        self.coreStoreURL = coreStoreURL
        self.authoritativeTranscriptReader = SharedTranscriptReader(facade: facade)
        self.authoritativeChapterReader = SharedChapterReader(facade: facade)
        let playbackHost = DeferredPlaybackHost()
        let agentHost = DeferredAgentHost()
        let recallHost = DeferredRecallHost()
        self.deferredPlaybackHost = playbackHost
        self.deferredAgentHost = agentHost
        self.deferredRecallHost = recallHost
        self.dispatcher = Pod0NativeHostDispatcher(
            feedHost: feedHost,
            downloadHost: downloadHost,
            notificationHost: notificationHost,
            agentHost: agentHost,
            playbackHost: playbackHost,
            recallHost: recallHost,
            observationOutbox: observationOutbox
        )
        self.dispatcher.bindDownloadOrphanObservations(to: facade)
    }

    func start() {
        guard subscriber == nil else { return }
        dispatcher.activateExecution()
        CoreDownloadEnvironmentMonitor.shared.start(client: self)
        let subscriber = SharedLibrarySubscriber { [weak self] projection in
            Task { @MainActor [weak self] in self?.receive(projection) }
        }
        self.subscriber = subscriber
        let facade = facade
        let commandExecutor = commandExecutor
        subscriptionTask = Task { @MainActor [weak self] in
            let subscriptions = await commandExecutor.makeSubscriptions(
                facade: facade,
                subscriber: subscriber
            )
            guard !Task.isCancelled, let self, self.subscriber === subscriber else {
                Task {
                    await commandExecutor.unsubscribe(subscriptions.ids, from: facade)
                }
                return
            }
            install(subscriptions)
            ensureNostrSigner()
            dispatcher.executePendingRequests(from: facade)
        }
    }

    func attach(store: AppStateStore) {
        self.store = store
        refreshInitialProjections()
    }

    private func receive(_ envelope: ProjectionEnvelope) {
        switch envelope.projection {
        case .library:
            receiveLibrary(envelope)
        case .playback(let projection):
            receivePlayback(projection, revision: envelope.stateRevision.value)
        case .recallConfiguration(let configuration):
            cachedRecallConfiguration = configuration
            store?.applySharedRecallConfiguration(configuration)
        case .chapterWorkflows(let projection):
            receiveChapterWorkflows(
                projection,
                revision: envelope.stateRevision.value
            )
        case .notes:
            receiveNotes(revision: envelope.stateRevision.value)
        case .memories:
            receiveMemories(revision: envelope.stateRevision.value)
        case .clips:
            receiveClips(revision: envelope.stateRevision.value)
        case .downloads:
            receiveDownloads(revision: envelope.stateRevision.value)
        case .transcriptWorkflows:
            receiveTranscriptWorkflows(revision: envelope.stateRevision.value)
        case .scheduledAgent(let projection):
            receiveScheduledAgents(projection, revision: envelope.stateRevision.value)
        case .nostrSigner(let projection):
            receiveNostrSigner(projection, revision: envelope.stateRevision.value)
        case .newEpisodeNotificationSettings(let projection):
            cachedNewEpisodeNotificationSettings = projection
            if let store { publishNewEpisodeNotificationSettings(to: store) }
        case .podcastDetail, .episodeDetail,
             .recall, .evidenceIndex, .transcript, .chapter, .agentConversations,
             .agentConversation, .publications, .unsupported:
            break
        }
    }

    func shutdown() {
        coreCommandTail?.cancel()
        coreCommandTail = nil
        coreCommandGeneration &+= 1
        subscriptionTask?.cancel()
        subscriptionTask = nil
        initialProjectionTask?.cancel()
        initialProjectionTask = nil
        libraryProjectionTask?.cancel()
        libraryProjectionTask = nil
        noteProjectionTask?.cancel()
        noteProjectionTask = nil
        memoryProjectionTask?.cancel()
        memoryProjectionTask = nil
        clipProjectionTask?.cancel()
        clipProjectionTask = nil
        scheduledAgentProjectionTask?.cancel()
        scheduledAgentProjectionTask = nil
        downloadProjectionTask?.cancel()
        downloadProjectionTask = nil
        evidenceRebuildTask?.cancel()
        evidenceRebuildTask = nil
        for task in evidenceUpdateTasks.values { task.cancel() }
        evidenceUpdateTasks.removeAll()
        for task in chapterProjectionTasks.values { task.cancel() }
        chapterProjectionTasks.removeAll()
        cancelAllRecallWaiters()
        dispatcher.shutdown()
        let subscriptionIDs = [
            librarySubscriptionID,
            playbackSubscriptionID,
            recallConfigurationSubscriptionID,
            chapterWorkflowSubscriptionID,
            notesSubscriptionID,
            memoriesSubscriptionID,
            clipsSubscriptionID,
            downloadsSubscriptionID,
            transcriptWorkflowSubscriptionID,
            newEpisodeNotificationSettingsSubscriptionID,
            scheduledAgentSubscriptionID,
            nostrSignerSubscriptionID,
        ].compactMap { $0 }
        if !subscriptionIDs.isEmpty {
            let commandExecutor = commandExecutor
            let facade = facade
            Task {
                await commandExecutor.unsubscribe(subscriptionIDs, from: facade)
            }
        }
        librarySubscriptionID = nil
        playbackSubscriptionID = nil
        recallConfigurationSubscriptionID = nil
        chapterWorkflowSubscriptionID = nil
        notesSubscriptionID = nil
        memoriesSubscriptionID = nil
        clipsSubscriptionID = nil
        downloadsSubscriptionID = nil
        transcriptWorkflowSubscriptionID = nil
        newEpisodeNotificationSettingsSubscriptionID = nil
        scheduledAgentSubscriptionID = nil
        nostrSignerSubscriptionID = nil
        cachedNostrSigner = nil
        cachedScheduledAgent = nil
        chapterScopes.removeAll()
        chapterSnapshots.removeAll()
        announcedPublisherChapterEpisodeIDs.removeAll()
        announcedModelChapterVersions.removeAll()
        cachedPublisherChapterWorkflows.removeAll()
        announcedTranscriptWorkflowVersions.removeAll()
        workflowClient?.detachPublisherChapterCore()
        workflowClient?.detachModelChapterCore()
        workflowClient?.detachDownloadCore()
        workflowClient?.detachTranscriptCore()
        workflowClient?.detachScheduledAgentCore()
        playbackChapterEpisodeID = nil
        subscriber = nil
        for waiter in waiters.values {
            waiter.continuation.resume(throwing: SharedLibraryError.cancelled)
        }
        waiters.removeAll()
    }
}
