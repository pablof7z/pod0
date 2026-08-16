import Foundation
import Pod0Core

/// Drains typed core effects after a command and returns correlated evidence.
/// It is event-driven: callers invoke `executePendingRequests` after dispatch;
/// no timer polls the facade.
@MainActor
final class Pod0NativeHostDispatcher {
    typealias Delivery = @MainActor (HostObservationEnvelope) -> Void

    let feedHost: any CoreFeedHosting
    let libraryNetworkHost: CoreLibraryNetworkHost
    let downloadHost: any CoreDownloadHosting
    let notificationHost: any CoreNotificationHosting
    let publisherChapterHost: any CorePublisherChapterHosting
    let chapterModelHost: any CoreChapterModelHosting
    let agentHost: any CoreAgentHosting
    let playbackHost: any CorePlaybackHosting
    let maximumConcurrentTasks: Int
    let recallHost: any CoreRecallHosting
    let scheduledAgentHost: any CoreScheduledAgentHosting
    let transcriptHost: any CoreTranscriptHosting
    let hostRequestReader = CoreHostRequestReader()
    let durableObservationRecorder: CoreDurableObservationRecorder
    let observationOutbox: NativeHostObservationOutbox?
    let now: @MainActor () -> Date
    var activeTasks: [HostRequestId: ActiveTask] = [:]
    var playbackStreams: [HostRequestId: PlaybackStream] = [:]
    var pendingScheduledAgentExecutions: [HostRequestId: PendingScheduledAgentExecution] = [:]
    var downloadRequests: [HostRequestId: ActiveDownloadRequest] = [:]
    var observationRecoveryTask: Task<Void, Never>?
    var observationRecoveryReady: Bool
    var completedRequestIDs: Set<HostRequestId> = []
    var completionOrder: [HostRequestId] = []
    var requestDrainTask: Task<Void, Never>?
    var requestDrainRequested = false
    var executionEnabled = false
    init(
        feedHost: any CoreFeedHosting,
        downloadHost: any CoreDownloadHosting = UnavailableCoreDownloadHost(),
        notificationHost: any CoreNotificationHosting = UnavailableCoreNotificationHost(),
        publisherChapterHost: any CorePublisherChapterHosting = CorePublisherChapterHost(),
        chapterModelHost: any CoreChapterModelHosting = CoreChapterModelHost(),
        agentHost: any CoreAgentHosting = UnavailableCoreAgentHost(),
        playbackHost: any CorePlaybackHosting,
        recallHost: any CoreRecallHosting = UnavailableCoreRecallHost(),
        scheduledAgentHost: any CoreScheduledAgentHosting = CoreScheduledAgentHost(),
        transcriptHost: any CoreTranscriptHosting = CoreTranscriptHost(),
        maximumConcurrentTasks: Int = 8,
        now: @escaping @MainActor () -> Date = Date.init,
        observationOutbox: NativeHostObservationOutbox? = nil
    ) {
        self.feedHost = feedHost
        self.libraryNetworkHost = CoreLibraryNetworkHost()
        self.downloadHost = downloadHost
        self.notificationHost = notificationHost
        self.publisherChapterHost = publisherChapterHost
        self.chapterModelHost = chapterModelHost
        self.agentHost = agentHost
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

    func activateExecution() {
        executionEnabled = true
    }

    /// Raw envelope execution is intentionally reachable only from the leased
    /// adapter. The architecture ratchet rejects every other production call.
    func executePersistedLeaseRequest(
        _ envelope: HostRequestEnvelope,
        delivery: @escaping Delivery
    ) {
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
        case .cancelAuthorizedEffect(let targetRequestID):
            cancel(
                requestID: targetRequestID,
                cancellationID: envelope.cancellationId
            )
            finish(
                envelope,
                sequenceNumber: 0,
                observation: .authorizedEffectCancellationApplied(
                    targetRequestId: targetRequestID
                ),
                delivery: delivery
            )
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
        case .fetchLibraryDocument(
            let workflowCommandID,
            let step,
            let url,
            let accept,
            let maximumResponseBytes
        ):
            startLibraryNetworkTask(
                envelope,
                workflowCommandID: workflowCommandID,
                step: step,
                url: url,
                accept: accept,
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
        case let .deliverNewEpisodeNotification(
            occurrenceID,
            episodeID,
            podcastID,
            podcastTitle,
            episodeTitle
        ):
            startNotificationTask(
                envelope,
                occurrenceID: occurrenceID,
                episodeID: episodeID,
                podcastID: podcastID,
                podcastTitle: podcastTitle,
                episodeTitle: episodeTitle,
                delivery: delivery
            )
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
