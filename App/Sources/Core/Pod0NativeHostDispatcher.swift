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

    struct PendingScheduledAgentExecution {
        let envelope: HostRequestEnvelope
        let execution: ScheduledAgentExecutionRequest
        let delivery: Delivery
    }

    struct AcknowledgementTask {
        let envelope: HostRequestEnvelope
        let observation: HostObservationEnvelope
        let completion: @MainActor () -> Void
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

    let feedHost: any CoreFeedHosting
    let downloadHost: any CoreDownloadHosting
    let publisherChapterHost: any CorePublisherChapterHosting
    let chapterModelHost: any CoreChapterModelHosting
    let agentHost: any CoreAgentHosting
    let nostrSignerHost: any CoreNostrSignerHosting
    let playbackHost: any CorePlaybackHosting
    private let maximumConcurrentTasks: Int
    let recallHost: any CoreRecallHosting
    let scheduledAgentHost: any CoreScheduledAgentHosting
    let transcriptHost: any CoreTranscriptHosting
    let recallObservationRecorder = CoreRecallObservationRecorder()
    let publisherObservationRecorder = CorePublisherChapterObservationRecorder()
    let durableObservationRecorder: CoreDurableObservationRecorder
    let observationOutbox: NativeHostObservationOutbox?
    let now: @MainActor () -> Date
    var activeTasks: [HostRequestId: ActiveTask] = [:]
    var playbackStreams: [HostRequestId: PlaybackStream] = [:]
    var acknowledgementTasks: [HostRequestId: AcknowledgementTask] = [:]
    var scheduledAgentAcknowledgementTasks: [HostRequestId: Task<Void, Never>] = [:]
    var pendingScheduledAgentObservations: [HostRequestId: [HostObservationEnvelope]] = [:]
    var pendingScheduledAgentExecutions: [HostRequestId: PendingScheduledAgentExecution] = [:]
    var scheduledAgentObservationCompletions: [HostRequestId: @MainActor () -> Void] = [:]
    var retainedScheduledAgentObservationIDs: Set<HostRequestId> = []
    var retainedObservationIDs: Set<HostRequestId> = []
    var retainedObservationRetryTask: Task<Void, Never>?
    var downloadAcknowledgementTasks: [HostRequestId: Task<Void, Never>] = [:]
    var downloadRequests: [HostRequestId: ActiveDownloadRequest] = [:]
    var pendingDownloadObservations: [HostRequestId: [HostObservationEnvelope]] = [:]
    var observationRecoveryTask: Task<Void, Never>?
    var observationRecoveryReady: Bool
    var completedRequestIDs: Set<HostRequestId> = []
    var completionOrder: [HostRequestId] = []
    private var executionEnabled = false
    init(
        feedHost: any CoreFeedHosting,
        downloadHost: any CoreDownloadHosting = UnavailableCoreDownloadHost(),
        publisherChapterHost: any CorePublisherChapterHosting = CorePublisherChapterHost(),
        chapterModelHost: any CoreChapterModelHosting = CoreChapterModelHost(),
        agentHost: any CoreAgentHosting = UnavailableCoreAgentHost(),
        nostrSignerHost: any CoreNostrSignerHosting = CoreNostrSignerHost(),
        playbackHost: any CorePlaybackHosting,
        recallHost: any CoreRecallHosting = UnavailableCoreRecallHost(),
        scheduledAgentHost: any CoreScheduledAgentHosting = CoreScheduledAgentHost(),
        transcriptHost: any CoreTranscriptHosting = CoreTranscriptHost(),
        maximumConcurrentTasks: Int = 8,
        now: @escaping @MainActor () -> Date = Date.init,
        observationOutbox: NativeHostObservationOutbox? = nil
    ) {
        self.feedHost = feedHost
        self.downloadHost = downloadHost
        self.publisherChapterHost = publisherChapterHost
        self.chapterModelHost = chapterModelHost
        self.agentHost = agentHost
        self.nostrSignerHost = nostrSignerHost
        self.playbackHost = playbackHost
        self.recallHost = recallHost
        self.scheduledAgentHost = scheduledAgentHost
        self.transcriptHost = transcriptHost
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
        guard executionEnabled else { return }
        guard observationRecoveryReady else {
            startObservationRecovery(from: facade, maximumCount: maximumCount)
            return
        }
        if retryRetainedObservations(in: facade) { return }
        if retryRetainedScheduledAgentObservations(in: facade) { return }
        for cancellation in facade.nextHostCancellations(maximumCount: maximumCount) {
            cancel(
                requestID: cancellation.requestId,
                cancellationID: cancellation.cancellationId
            )
        }
        let capacity = max(
            0,
            maximumConcurrentTasks - activeTasks.count - acknowledgementTasks.count
                - downloadRequests.count - scheduledAgentAcknowledgementTasks.count
                - pendingScheduledAgentExecutions.count
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

    func activateExecution() {
        executionEnabled = true
    }

    func execute(_ envelope: HostRequestEnvelope, delivery: @escaping Delivery) {
        guard !isKnown(envelope.requestId) else { return }
        guard !isExpired(envelope) else {
            let observation: HostObservation
            if case .executeScheduledAgentTurn(let execution) = envelope.request {
                observation = .scheduledAgentExecutionObserved(
                    observation: expiredScheduledAgentObservation(execution)
                )
            } else {
                observation = .failed(
                    code: .timedOut,
                    safeDetail: "Host request deadline expired"
                )
            }
            finish(
                envelope,
                sequenceNumber: 0,
                observation: observation,
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
        case .executeTranscriptCapability:
            guard observationOutbox != nil else {
                finish(
                    envelope,
                    sequenceNumber: 0,
                    observation: .failed(
                        code: .platformFailure,
                        safeDetail: "Durable transcript observation staging is unavailable"
                    ),
                    delivery: delivery,
                    remember: false
                )
                return
            }
            startTranscriptTask(envelope, delivery: delivery)
        case .executeScheduledAgentTurn(let execution):
            guard observationOutbox != nil else {
                finish(
                    envelope,
                    sequenceNumber: 0,
                    observation: .scheduledAgentExecutionObserved(observation: .failed(
                        occurrenceId: execution.occurrenceId,
                        attemptId: execution.attemptId,
                        code: .storageUnavailable,
                        safeDetail: "Durable scheduled-agent observation staging is unavailable",
                        retryAfterMilliseconds: nil
                    )),
                    delivery: delivery,
                    remember: false
                )
                return
            }
            startScheduledAgentTask(
                envelope,
                execution: execution,
                delivery: delivery
            )
        case .executeAgentModelTurn, .presentAgentApproval, .executeAgentCapability:
            guard observationOutbox != nil else {
                finish(
                    envelope,
                    sequenceNumber: 0,
                    observation: .failed(
                        code: .platformFailure,
                        safeDetail: "Durable agent observation staging is unavailable"
                    ),
                    delivery: delivery,
                    remember: false
                )
                return
            }
            startAgentTask(envelope, delivery: delivery)
        case .provisionNostrSignerCredential, .restoreNostrSignerCredential,
             .signNostrEvent, .deleteNostrSignerCredential:
            startNostrSignerTask(envelope, delivery: delivery)
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

}
