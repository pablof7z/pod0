import Foundation
import Pod0Core

/// Drains typed core effects after a command and returns correlated evidence.
/// It is event-driven: callers invoke `executePendingRequests` after dispatch;
/// no timer polls the facade.
@MainActor
final class Pod0NativeHostDispatcher {
    typealias Delivery = @MainActor (HostObservationEnvelope) -> Void

    struct ActiveTask {
        let envelope: HostRequestEnvelope
        let task: Task<Void, Never>
        let delivery: Delivery
    }

    struct AcknowledgementTask {
        let envelope: HostRequestEnvelope
        let task: Task<Void, Never>
    }

    struct PlaybackStream {
        let envelope: HostRequestEnvelope
        let episodeID: EpisodeId?
        let minimumInterval: TimeInterval
        let delivery: Delivery
        var sequenceNumber: UInt64 = 0
        var lastDeliveryAt: Date?
        var lastObservation: PlaybackLifecycleObservation?
    }

    private let feedHost: any CoreFeedHosting
    let downloadHost: any CoreDownloadHosting
    let publisherChapterHost: any CorePublisherChapterHosting
    let chapterModelHost: any CoreChapterModelHosting
    let playbackHost: any CorePlaybackHosting
    private let maximumConcurrentTasks: Int
    let recallHost: any CoreRecallHosting
    let recallObservationRecorder = CoreRecallObservationRecorder()
    let publisherObservationRecorder = CorePublisherChapterObservationRecorder()
    let durableObservationRecorder: CoreDurableObservationRecorder
    let observationOutbox: NativeHostObservationOutbox?
    let now: @MainActor () -> Date
    var activeTasks: [HostRequestId: ActiveTask] = [:]
    var playbackStreams: [HostRequestId: PlaybackStream] = [:]
    var acknowledgementTasks: [HostRequestId: AcknowledgementTask] = [:]
    var downloadAcknowledgementTasks: [HostRequestId: Task<Void, Never>] = [:]
    var downloadRequests: [HostRequestId: ActiveDownloadRequest] = [:]
    var pendingDownloadObservations: [HostRequestId: [HostObservationEnvelope]] = [:]
    var observationRecoveryTask: Task<Void, Never>?
    var observationRecoveryReady: Bool
    private var completedRequestIDs: Set<HostRequestId> = []
    private var completionOrder: [HostRequestId] = []

    init(
        feedHost: any CoreFeedHosting,
        downloadHost: any CoreDownloadHosting = UnavailableCoreDownloadHost(),
        publisherChapterHost: any CorePublisherChapterHosting = CorePublisherChapterHost(),
        chapterModelHost: any CoreChapterModelHosting = CoreChapterModelHost(),
        playbackHost: any CorePlaybackHosting,
        recallHost: any CoreRecallHosting = UnavailableCoreRecallHost(),
        maximumConcurrentTasks: Int = 8,
        now: @escaping @MainActor () -> Date = Date.init,
        observationOutbox: NativeHostObservationOutbox? = nil
    ) {
        self.feedHost = feedHost
        self.downloadHost = downloadHost
        self.publisherChapterHost = publisherChapterHost
        self.chapterModelHost = chapterModelHost
        self.playbackHost = playbackHost
        self.recallHost = recallHost
        self.observationOutbox = observationOutbox
        self.durableObservationRecorder = CoreDurableObservationRecorder(
            outbox: observationOutbox
        )
        self.observationRecoveryReady = observationOutbox == nil
        self.maximumConcurrentTasks = max(1, maximumConcurrentTasks)
        self.now = now
        playbackHost.installObservationSink { [weak self] observation in
            self?.receivePlaybackObservation(observation)
        }
    }

    func executePendingRequests(from facade: Pod0Facade, maximumCount: UInt16 = 64) {
        guard observationRecoveryReady else {
            startObservationRecovery(from: facade, maximumCount: maximumCount)
            return
        }
        for cancellation in facade.nextHostCancellations(maximumCount: maximumCount) {
            cancel(
                requestID: cancellation.requestId,
                cancellationID: cancellation.cancellationId
            )
        }
        let capacity = max(
            0,
            maximumConcurrentTasks - activeTasks.count - acknowledgementTasks.count
                - downloadRequests.count
        )
        let boundedCount = min(Int(maximumCount), capacity)
        guard boundedCount > 0 else { return }
        for envelope in facade.nextHostRequests(maximumCount: UInt16(boundedCount)) {
            execute(envelope) { [weak self] observation in
                guard let self else { return }
                record(observation, for: envelope, in: facade) { [weak self] in
                    self?.executePendingRequests(from: facade, maximumCount: maximumCount)
                }
            }
        }
    }

    func execute(_ envelope: HostRequestEnvelope, delivery: @escaping Delivery) {
        guard !isKnown(envelope.requestId) else { return }
        guard !isExpired(envelope) else {
            finish(
                envelope,
                sequenceNumber: 0,
                observation: .failed(code: .timedOut, safeDetail: "Host request deadline expired"),
                delivery: delivery
            )
            return
        }

        switch envelope.request {
        case .fetchFeed(
            let feedURL,
            let entityTag,
            let lastModified,
            let maximumResponseBytes
        ):
            startFeedTask(
                envelope,
                feedURL: feedURL,
                entityTag: entityTag,
                lastModified: lastModified,
                maximumResponseBytes: maximumResponseBytes,
                delivery: delivery
            )
        case .observePlayback(let episodeID, let minimumIntervalMilliseconds):
            startPlaybackStream(
                envelope,
                episodeID: episodeID,
                minimumIntervalMilliseconds: minimumIntervalMilliseconds,
                delivery: delivery
            )
        case .fetchPublisherChapters(
            let episodeID,
            let sourceURL,
            let notBefore,
            let maximumResponseBytes
        ):
            startPublisherChapterTask(
                envelope,
                episodeID: episodeID,
                sourceURL: sourceURL,
                notBefore: notBefore,
                maximumResponseBytes: maximumResponseBytes,
                delivery: delivery
            )
        case .embedRecallQuery, .embedRecallSpans, .rerankRecallCandidates,
             .removeLegacyRecallIndexArtifacts:
            startRecallTask(envelope, delivery: delivery)
        case .executeChapterModel, .recoverChapterModelOperation:
            guard observationOutbox != nil else {
                finish(
                    envelope,
                    sequenceNumber: 0,
                    observation: .failed(
                        code: .platformFailure,
                        safeDetail: "Durable model observation staging is unavailable"
                    ),
                    delivery: delivery,
                    remember: false
                )
                return
            }
            startChapterModelTask(envelope, delivery: delivery)
        case .scheduleCoreWake(let wakeAt, let reason):
            startCoreWakeTask(
                envelope,
                wakeAt: wakeAt,
                reason: reason,
                delivery: delivery
            )
        case .startEpisodeDownload, .cancelEpisodeDownload,
             .removeEpisodeDownloadArtifact:
            startDownloadRequest(envelope, delivery: delivery)
        default:
            finish(
                envelope,
                sequenceNumber: 0,
                observation: playbackHost.execute(envelope.request),
                delivery: delivery
            )
        }
    }

    private func startFeedTask(
        _ envelope: HostRequestEnvelope,
        feedURL: String,
        entityTag: String?,
        lastModified: String?,
        maximumResponseBytes: UInt64,
        delivery: @escaping Delivery
    ) {
        let task = Task { @MainActor [weak self] in
            guard let self else { return }
            let result = await feedHost.fetch(
                feedURL: feedURL,
                entityTag: entityTag,
                lastModified: lastModified,
                maximumResponseBytes: maximumResponseBytes,
                deadline: envelope.deadlineAt?.date
            )
            guard activeTasks.removeValue(forKey: envelope.requestId) != nil else { return }
            let observation: HostObservation = isExpired(envelope)
                ? .failed(code: .timedOut, safeDetail: "Host request deadline expired")
                : result
            finish(
                envelope,
                sequenceNumber: 0,
                observation: observation,
                delivery: delivery
            )
        }
        activeTasks[envelope.requestId] = ActiveTask(
            envelope: envelope,
            task: task,
            delivery: delivery
        )
    }

    private func isKnown(_ requestID: HostRequestId) -> Bool {
        activeTasks[requestID] != nil
            || downloadRequests[requestID] != nil
            || playbackStreams[requestID] != nil
            || acknowledgementTasks[requestID] != nil
            || completedRequestIDs.contains(requestID)
    }

    func rememberCompletion(_ requestID: HostRequestId) {
        guard completedRequestIDs.insert(requestID).inserted else { return }
        completionOrder.append(requestID)
        if completionOrder.count > 256 {
            completedRequestIDs.remove(completionOrder.removeFirst())
        }
    }
}
