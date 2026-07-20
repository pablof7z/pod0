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
    let publisherChapterHost: any CorePublisherChapterHosting
    private let playbackHost: any CorePlaybackHosting
    private let maximumConcurrentTasks: Int
    let recallHost: any CoreRecallHosting
    let recallObservationRecorder = CoreRecallObservationRecorder()
    let publisherObservationRecorder = CorePublisherChapterObservationRecorder()
    let now: @MainActor () -> Date
    var activeTasks: [HostRequestId: ActiveTask] = [:]
    var playbackStreams: [HostRequestId: PlaybackStream] = [:]
    private var completedRequestIDs: Set<HostRequestId> = []
    private var completionOrder: [HostRequestId] = []

    init(
        feedHost: any CoreFeedHosting,
        publisherChapterHost: any CorePublisherChapterHosting = CorePublisherChapterHost(),
        playbackHost: any CorePlaybackHosting,
        recallHost: any CoreRecallHosting = UnavailableCoreRecallHost(),
        maximumConcurrentTasks: Int = 8,
        now: @escaping @MainActor () -> Date = Date.init
    ) {
        self.feedHost = feedHost
        self.publisherChapterHost = publisherChapterHost
        self.playbackHost = playbackHost
        self.recallHost = recallHost
        self.maximumConcurrentTasks = max(1, maximumConcurrentTasks)
        self.now = now
        playbackHost.installObservationSink { [weak self] observation in
            self?.receivePlaybackObservation(observation)
        }
    }

    func executePendingRequests(from facade: Pod0Facade, maximumCount: UInt16 = 64) {
        for cancellation in facade.nextHostCancellations(maximumCount: maximumCount) {
            cancel(
                requestID: cancellation.requestId,
                cancellationID: cancellation.cancellationId
            )
        }
        let capacity = max(0, maximumConcurrentTasks - activeTasks.count)
        let boundedCount = min(Int(maximumCount), capacity)
        guard boundedCount > 0 else { return }
        for envelope in facade.nextHostRequests(maximumCount: UInt16(boundedCount)) {
            execute(envelope) { [weak self] observation in
                guard let self else { return }
                record(observation, for: envelope.request, in: facade) { [weak self] in
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

    private func startPlaybackStream(
        _ envelope: HostRequestEnvelope,
        episodeID: EpisodeId?,
        minimumIntervalMilliseconds: UInt32,
        delivery: @escaping Delivery
    ) {
        let boundedMilliseconds = max(500, min(minimumIntervalMilliseconds, 5_000))
        playbackStreams[envelope.requestId] = PlaybackStream(
            envelope: envelope,
            episodeID: episodeID,
            minimumInterval: Double(boundedMilliseconds) / 1_000,
            delivery: delivery
        )
        if case .playbackObserved(let value) = playbackHost.execute(envelope.request) {
            receivePlaybackObservation(value)
        }
    }

    private func receivePlaybackObservation(_ observation: PlaybackLifecycleObservation) {
        let timestamp = now()
        for requestID in Array(playbackStreams.keys) {
            guard var stream = playbackStreams[requestID],
                  stream.episodeID == nil || stream.episodeID == observation.episodeId
            else { continue }
            let isBoundary = Self.isBoundary(
                previous: stream.lastObservation,
                current: observation
            )
            let intervalElapsed = stream.lastDeliveryAt.map {
                timestamp.timeIntervalSince($0) >= stream.minimumInterval
            } ?? true
            guard isBoundary || intervalElapsed else { continue }

            stream.sequenceNumber += 1
            stream.lastDeliveryAt = timestamp
            stream.lastObservation = observation
            playbackStreams[requestID] = stream
            stream.delivery(makeEnvelope(
                stream.envelope,
                sequenceNumber: stream.sequenceNumber,
                observedAt: timestamp,
                observation: .playbackObserved(value: observation)
            ))
        }
    }

    private static func isBoundary(
        previous: PlaybackLifecycleObservation?,
        current: PlaybackLifecycleObservation
    ) -> Bool {
        guard let previous else { return true }
        return previous.episodeId != current.episodeId
            || previous.state != current.state
            || previous.durationMilliseconds != current.durationMilliseconds
            || previous.route != current.route
            || previous.interruption != current.interruption
            || previous.ended != current.ended
    }

    func finish(
        _ envelope: HostRequestEnvelope,
        sequenceNumber: UInt64,
        observation: HostObservation,
        delivery: Delivery
    ) {
        rememberCompletion(envelope.requestId)
        delivery(makeEnvelope(
            envelope,
            sequenceNumber: sequenceNumber,
            observedAt: now(),
            observation: observation
        ))
    }

    private func makeEnvelope(
        _ request: HostRequestEnvelope,
        sequenceNumber: UInt64,
        observedAt: Date,
        observation: HostObservation
    ) -> HostObservationEnvelope {
        HostObservationEnvelope(
            requestId: request.requestId,
            cancellationId: request.cancellationId,
            observedRequestRevision: request.issuedRevision,
            sequenceNumber: sequenceNumber,
            observedAt: UnixTimestampMilliseconds(date: observedAt),
            observation: observation
        )
    }

    private func isKnown(_ requestID: HostRequestId) -> Bool {
        activeTasks[requestID] != nil
            || playbackStreams[requestID] != nil
            || completedRequestIDs.contains(requestID)
    }

    func isExpired(_ envelope: HostRequestEnvelope) -> Bool {
        envelope.deadlineAt.map { $0.date <= now() } ?? false
    }

    func rememberCompletion(_ requestID: HostRequestId) {
        guard completedRequestIDs.insert(requestID).inserted else { return }
        completionOrder.append(requestID)
        if completionOrder.count > 256 {
            completedRequestIDs.remove(completionOrder.removeFirst())
        }
    }
}
