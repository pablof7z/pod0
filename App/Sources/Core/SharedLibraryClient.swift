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
    private let deferredPlaybackHost: DeferredPlaybackHost
    let deferredRecallHost: DeferredRecallHost
    private var subscriber: SharedLibrarySubscriber?
    private var librarySubscriptionID: SubscriptionId?
    private var playbackSubscriptionID: SubscriptionId?
    private var chapterWorkflowSubscriptionID: SubscriptionId?
    private var notesSubscriptionID: SubscriptionId?
    private var clipsSubscriptionID: SubscriptionId?
    var waiters: [CommandId: Waiter] = [:]
    var lastLibraryRevision: UInt64 = 0
    private var lastPlaybackRevision: UInt64 = 0
    var lastChapterWorkflowRevision: UInt64 = 0
    var lastNotesRevision: UInt64 = 0
    var lastClipsRevision: UInt64 = 0
    weak var store: AppStateStore?
    private weak var playbackState: PlaybackState?
    var cachedSnapshot: SharedLibrarySnapshot?
    var chapterScopeCounts: [UUID: Int] = [:]
    var chapterSnapshots: [UUID: SharedChapterSnapshot] = [:]
    var announcedPublisherChapterEpisodeIDs: Set<UUID> = []
    var playbackChapterEpisodeID: UUID?
    private var cachedPlayback: PlaybackProjection?
    private var cachedPlaybackRevision: UInt64 = 0
    var cachedNotes: SharedNoteSnapshot?
    var cachedClips: SharedClipSnapshot?
    private var playbackHostAttached = false
    var evidenceRebuildTask: Task<Void, Never>?
    var evidenceUpdateTasks: [UUID: Task<Void, Never>] = [:]
    var recallWaiters: [RecallQueryId: SharedRecallWaiter] = [:]
    var rebuildingEvidenceEpisodeIDs: Set<UUID> = []
    var recallHostAttached = false
    weak var workflowClient: WorkflowClient?

    init(
        facade: Pod0Facade,
        feedHost: any CoreFeedHosting
    ) {
        self.facade = facade
        self.authoritativeTranscriptReader = SharedTranscriptReader(facade: facade)
        self.authoritativeChapterReader = SharedChapterReader(facade: facade)
        let playbackHost = DeferredPlaybackHost()
        let recallHost = DeferredRecallHost()
        self.deferredPlaybackHost = playbackHost
        self.deferredRecallHost = recallHost
        self.dispatcher = Pod0NativeHostDispatcher(
            feedHost: feedHost,
            playbackHost: playbackHost,
            recallHost: recallHost
        )
    }

    func start() {
        guard librarySubscriptionID == nil else { return }
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
        chapterWorkflowSubscriptionID = facade.subscribe(
            request: ProjectionRequest(
                scope: .chapterWorkflows(episodeId: nil),
                offset: 0,
                maxItems: 1
            ),
            subscriber: subscriber
        )
        notesSubscriptionID = facade.subscribe(
            request: ProjectionRequest(scope: .notes(scope: .all), offset: 0, maxItems: 200),
            subscriber: subscriber
        )
        clipsSubscriptionID = facade.subscribe(
            request: ProjectionRequest(scope: .clips(scope: .active), offset: 0, maxItems: 200),
            subscriber: subscriber
        )
    }

    func attach(store: AppStateStore) {
        self.store = store
        let snapshot = loadAllPages()
        cachedSnapshot = snapshot
        store.applySharedLibrary(snapshot)
        let notes = loadNotePages(scope: .all)
        cachedNotes = notes
        store.applySharedNotes(notes)
        let clips = loadClipPages(scope: .active)
        cachedClips = clips
        store.applySharedClips(clips)
    }

    nonisolated func chapterModelPlan(
        episodeID: UUID,
        configuredModel: String
    ) -> ChapterModelPlan {
        facade.planChapterModelRequest(
            episodeId: EpisodeId(uuid: episodeID),
            configuredModel: configuredModel
        )
    }

    func attachPlayback(_ playback: PlaybackState, store: AppStateStore) {
        self.playbackState = playback
        playback.attachSharedCore(self)
        if !playbackHostAttached {
            deferredPlaybackHost.attach(CorePlaybackHost(
                engine: playback.engine,
                resolveEpisode: { [weak store] id in store?.episode(id: id) }
            ))
            playbackHostAttached = true
        }
        if let cachedPlayback {
            playback.applySharedPlayback(
                cachedPlayback,
                stateRevision: cachedPlaybackRevision
            ) { [weak store] id in
                store?.episode(id: id)
            }
        }
        dispatchPlayback(.restore)
    }

    func dispatchPlayback(_ command: PlaybackCommand) {
        facade.dispatch(command: CommandEnvelope(
            commandId: CommandId(uuid: UUID()),
            cancellationId: CancellationId(uuid: UUID()),
            expectedRevision: nil,
            command: .playback(command: command)
        ))
        dispatcher.executePendingRequests(from: facade)
    }

    func executePendingHostRequests() {
        dispatcher.executePendingRequests(from: facade)
    }

    func cancelPendingHostRequests(cancellationID: CancellationId) {
        dispatcher.cancel(cancellationID: cancellationID)
    }

    func execute(_ command: ApplicationCommand) async throws -> OperationResult? {
        let commandID = CommandId(uuid: UUID())
        let cancellationID = CancellationId(uuid: UUID())
        return try await withCheckedThrowingContinuation { continuation in
            waiters[commandID] = Waiter(continuation: continuation)
            facade.dispatch(command: CommandEnvelope(
                commandId: commandID,
                cancellationId: cancellationID,
                expectedRevision: nil,
                command: command
            ))
            dispatcher.executePendingRequests(from: facade)
        }
    }

    func podcast(id: UUID) -> Podcast? {
        cachedSnapshot?.podcasts.first { $0.podcastId.uuid == id }?.swiftValue
    }

    func podcast(feedURL: URL) -> Podcast? {
        let key = feedURL.absoluteString.lowercased()
        return cachedSnapshot?.podcasts.first {
            $0.feedIdentity?.comparisonKey == key
        }?.swiftValue
    }

    func subscription(podcastID: UUID) -> PodcastSubscription? {
        cachedSnapshot?.subscriptions.first {
            $0.podcastId.uuid == podcastID
        }?.swiftValue
    }

    private func receive(_ envelope: ProjectionEnvelope) {
        switch envelope.projection {
        case .library:
            receiveLibrary(envelope)
        case .playback(let projection):
            receivePlayback(projection, revision: envelope.stateRevision.value)
        case .chapterWorkflows:
            receivePublisherChapterWorkflows(revision: envelope.stateRevision.value)
        case .notes:
            receiveNotes(revision: envelope.stateRevision.value)
        case .clips:
            receiveClips(revision: envelope.stateRevision.value)
        case .podcastDetail, .episodeDetail, .recall, .evidenceIndex, .transcript, .chapter,
             .unsupported:
            break
        }
    }

    private func receivePlayback(_ projection: PlaybackProjection, revision: UInt64) {
        guard revision >= lastPlaybackRevision else { return }
        lastPlaybackRevision = revision
        cachedPlayback = projection
        cachedPlaybackRevision = revision
        let projectedEpisodeID = projection.current?.episodeId.uuid
        if playbackChapterEpisodeID != projectedEpisodeID {
            if let playbackChapterEpisodeID {
                releaseChapterProjection(episodeID: playbackChapterEpisodeID)
            }
            playbackChapterEpisodeID = projectedEpisodeID
            if let projectedEpisodeID {
                retainChapterProjection(episodeID: projectedEpisodeID)
            }
        }
        if let playbackState {
            playbackState.applySharedPlayback(
                projection,
                stateRevision: revision
            ) { [weak store] id in
                store?.episode(id: id)
            }
        }
        dispatcher.executePendingRequests(from: facade)
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
        if let chapterWorkflowSubscriptionID {
            facade.unsubscribe(subscriptionId: chapterWorkflowSubscriptionID)
        }
        if let notesSubscriptionID { facade.unsubscribe(subscriptionId: notesSubscriptionID) }
        if let clipsSubscriptionID { facade.unsubscribe(subscriptionId: clipsSubscriptionID) }
        librarySubscriptionID = nil
        playbackSubscriptionID = nil
        chapterWorkflowSubscriptionID = nil
        notesSubscriptionID = nil
        clipsSubscriptionID = nil
        chapterScopeCounts.removeAll()
        chapterSnapshots.removeAll()
        announcedPublisherChapterEpisodeIDs.removeAll()
        workflowClient?.detachPublisherChapterCore()
        playbackChapterEpisodeID = nil
        subscriber = nil
        for waiter in waiters.values {
            waiter.continuation.resume(throwing: SharedLibraryError.cancelled)
        }
        waiters.removeAll()
    }

}

private final class SharedLibrarySubscriber: ProjectionSubscriber, @unchecked Sendable {
    private let delivery: @Sendable (ProjectionEnvelope) -> Void

    init(delivery: @escaping @Sendable (ProjectionEnvelope) -> Void) {
        self.delivery = delivery
    }

    func receive(projection: ProjectionEnvelope) {
        delivery(projection)
    }
}
