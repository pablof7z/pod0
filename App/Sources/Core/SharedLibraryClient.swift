import Foundation
import Pod0Core

@MainActor
final class SharedLibraryClient {
    static let maximumActiveChapterProjections = 8
    struct Waiter {
        let continuation: CheckedContinuation<OperationResult?, Error>
    }

    nonisolated let facade: Pod0Facade
    let authoritativeTranscriptReader: SharedTranscriptReader
    let authoritativeChapterReader: SharedChapterReader
    let dispatcher: Pod0NativeHostDispatcher
    let agentStreamingState = CoreAgentStreamingState()
    let deferredPlaybackHost: DeferredPlaybackHost
    let deferredAgentHost: DeferredAgentHost
    let deferredRecallHost: DeferredRecallHost
    private var subscriber: SharedLibrarySubscriber?
    private var librarySubscriptionID: SubscriptionId?
    private var playbackSubscriptionID: SubscriptionId?
    private var chapterWorkflowSubscriptionID: SubscriptionId?
    var recallConfigurationSubscriptionID: SubscriptionId?
    private var notesSubscriptionID: SubscriptionId?
    private var memoriesSubscriptionID: SubscriptionId?
    private var clipsSubscriptionID: SubscriptionId?
    private var downloadsSubscriptionID: SubscriptionId?
    private var transcriptWorkflowSubscriptionID: SubscriptionId?
    private var nostrSignerSubscriptionID: SubscriptionId?
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
    var chapterScopeCounts: [UUID: Int] = [:]
    var chapterSnapshots: [UUID: SharedChapterSnapshot] = [:]
    var announcedPublisherChapterEpisodeIDs: Set<UUID> = []
    var announcedModelChapterVersions: [UUID: String] = [:]
    var cachedPublisherChapterWorkflows: [PublisherChapterWorkflowProjection] = []
    var playbackChapterEpisodeID: UUID?
    var cachedPlayback: PlaybackProjection?
    var cachedPlaybackRevision: UInt64 = 0
    var cachedNotes: SharedNoteSnapshot?
    var cachedMemories: SharedMemorySnapshot?
    var cachedClips: SharedClipSnapshot?
    var lastDownloadsRevision: UInt64 = 0
    var cachedDownloadWorkflows: [UUID: DownloadWorkflowProjection] = [:]
    var lastTranscriptWorkflowRevision: UInt64 = 0
    var lastScheduledAgentRevision: UInt64 = 0
    var lastNostrSignerRevision: UInt64 = 0
    var cachedNostrSigner: SignerProjection?
    var cachedScheduledAgent: ScheduledAgentProjection?
    var announcedTranscriptWorkflowVersions: [UUID: String] = [:]
    var playbackHostAttached = false
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
        // #160 enables the live host only when legacy notification writes retire.
        notificationHost: any CoreNotificationHosting = UnavailableCoreNotificationHost(),
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
        guard librarySubscriptionID == nil else { return }
        dispatcher.activateExecution()
        CoreDownloadEnvironmentMonitor.shared.start(client: self)
        let subscriber = SharedLibrarySubscriber { [weak self] projection in
            Task { @MainActor [weak self] in self?.receive(projection) }
        }
        self.subscriber = subscriber
        librarySubscriptionID = facade.subscribe(
            request: ProjectionRequest(scope: .library, offset: 0, maxItems: 200),
            subscriber: subscriber
        )
        playbackSubscriptionID = facade.subscribe(
            request: ProjectionRequest(scope: .playback, offset: 0, maxItems: 200),
            subscriber: subscriber
        )
        subscribeToRecallConfiguration(subscriber)
        chapterWorkflowSubscriptionID = facade.subscribe(
            request: ProjectionRequest(
                scope: .chapterWorkflows(episodeId: nil),
                offset: 0,
                maxItems: 200
            ),
            subscriber: subscriber
        )
        notesSubscriptionID = facade.subscribe(
            request: ProjectionRequest(scope: .notes(scope: .all), offset: 0, maxItems: 200),
            subscriber: subscriber
        )
        memoriesSubscriptionID = facade.subscribe(
            request: ProjectionRequest(scope: .memories(scope: .all), offset: 0, maxItems: 200),
            subscriber: subscriber
        )
        clipsSubscriptionID = facade.subscribe(
            request: ProjectionRequest(scope: .clips(scope: .active), offset: 0, maxItems: 200),
            subscriber: subscriber
        )
        downloadsSubscriptionID = facade.subscribe(
            request: ProjectionRequest(
                scope: .downloads(episodeId: nil),
                offset: 0,
                maxItems: 200
            ),
            subscriber: subscriber
        )
        transcriptWorkflowSubscriptionID = facade.subscribe(
            request: ProjectionRequest(
                scope: .transcriptWorkflows(episodeId: nil),
                offset: 0,
                maxItems: 200
            ),
            subscriber: subscriber
        )
        subscribeToScheduledAgents(subscriber)
        nostrSignerSubscriptionID = facade.subscribe(
            request: ProjectionRequest(scope: .nostrSigner, offset: 0, maxItems: 20),
            subscriber: subscriber
        )
        ensureNostrSigner()
        dispatcher.executePendingRequests(from: facade)
    }

    func attach(store: AppStateStore) {
        self.store = store
        let snapshot = loadAllPages()
        cachedSnapshot = snapshot
        store.applySharedLibrary(snapshot)
        let notes = loadNotePages(scope: .all)
        cachedNotes = notes
        store.applySharedNotes(notes)
        let memories = loadMemoryPages(scope: .all)
        cachedMemories = memories
        store.applySharedMemories(memories)
        let clips = loadClipPages(scope: .active)
        cachedClips = clips
        store.applySharedClips(clips)
        publishScheduledAgents(to: store)
        publishRecallConfiguration(to: store)
    }

    private func receive(_ envelope: ProjectionEnvelope) {
        switch envelope.projection {
        case .library:
            receiveLibrary(envelope)
        case .playback(let projection):
            receivePlayback(projection, revision: envelope.stateRevision.value)
        case .recallConfiguration(let configuration):
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
        case .podcastDetail, .episodeDetail, .newEpisodeNotificationSettings,
             .recall, .evidenceIndex, .transcript, .chapter, .agentConversations,
             .agentConversation, .publications, .unsupported:
            break
        }
    }

    func shutdown() {
        evidenceRebuildTask?.cancel()
        evidenceRebuildTask = nil
        for task in evidenceUpdateTasks.values { task.cancel() }
        evidenceUpdateTasks.removeAll()
        cancelAllRecallWaiters()
        dispatcher.shutdown()
        if let librarySubscriptionID { facade.unsubscribe(subscriptionId: librarySubscriptionID) }
        if let playbackSubscriptionID { facade.unsubscribe(subscriptionId: playbackSubscriptionID) }
        unsubscribeFromRecallConfiguration()
        if let chapterWorkflowSubscriptionID {
            facade.unsubscribe(subscriptionId: chapterWorkflowSubscriptionID)
        }
        if let notesSubscriptionID { facade.unsubscribe(subscriptionId: notesSubscriptionID) }
        if let memoriesSubscriptionID {
            facade.unsubscribe(subscriptionId: memoriesSubscriptionID)
        }
        if let clipsSubscriptionID { facade.unsubscribe(subscriptionId: clipsSubscriptionID) }
        if let downloadsSubscriptionID {
            facade.unsubscribe(subscriptionId: downloadsSubscriptionID)
        }
        if let transcriptWorkflowSubscriptionID {
            facade.unsubscribe(subscriptionId: transcriptWorkflowSubscriptionID)
        }
        unsubscribeFromScheduledAgents()
        if let nostrSignerSubscriptionID {
            facade.unsubscribe(subscriptionId: nostrSignerSubscriptionID)
        }
        librarySubscriptionID = nil
        playbackSubscriptionID = nil
        chapterWorkflowSubscriptionID = nil
        notesSubscriptionID = nil
        memoriesSubscriptionID = nil
        clipsSubscriptionID = nil
        downloadsSubscriptionID = nil
        transcriptWorkflowSubscriptionID = nil
        nostrSignerSubscriptionID = nil
        cachedNostrSigner = nil
        chapterScopeCounts.removeAll()
        chapterSnapshots.removeAll()
        announcedPublisherChapterEpisodeIDs.removeAll()
        announcedModelChapterVersions.removeAll()
        cachedPublisherChapterWorkflows.removeAll()
        announcedTranscriptWorkflowVersions.removeAll()
        workflowClient?.detachPublisherChapterCore()
        workflowClient?.detachModelChapterCore()
        workflowClient?.detachDownloadCore()
        workflowClient?.detachTranscriptCore()
        playbackChapterEpisodeID = nil
        subscriber = nil
        for waiter in waiters.values {
            waiter.continuation.resume(throwing: SharedLibraryError.cancelled)
        }
        waiters.removeAll()
    }
}
