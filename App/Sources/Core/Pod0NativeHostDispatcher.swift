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
    private let playbackHost: any CorePlaybackHosting
    let recallHost: any CoreRecallHosting
    private let now: @MainActor () -> Date
    var activeTasks: [HostRequestId: ActiveTask] = [:]
    var playbackStreams: [HostRequestId: PlaybackStream] = [:]
    private var completedRequestIDs: Set<HostRequestId> = []
    private var completionOrder: [HostRequestId] = []

    init(
        feedHost: any CoreFeedHosting,
        playbackHost: any CorePlaybackHosting,
        recallHost: any CoreRecallHosting = UnavailableCoreRecallHost(),
        now: @escaping @MainActor () -> Date = Date.init
    ) {
        self.feedHost = feedHost
        self.playbackHost = playbackHost
        self.recallHost = recallHost
        self.now = now
        playbackHost.installObservationSink { [weak self] observation in
            self?.receivePlaybackObservation(observation)
        }
    }

    func executePendingRequests(from facade: Pod0Facade, maximumCount: UInt16 = 64) {
        for envelope in facade.nextHostRequests(maximumCount: maximumCount) {
            execute(envelope) { observation in
                facade.recordHostObservation(observation: observation)
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
        case .embedRecallQuery, .retrieveRecallCandidates,
             .rerankRecallCandidates, .rebuildRecallIndex:
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

    func cancel(cancellationID: CancellationId) {
        let taskIDs = activeTasks.compactMap { requestID, active in
            active.envelope.cancellationId == cancellationID ? requestID : nil
        }
        for requestID in taskIDs {
            guard let active = activeTasks.removeValue(forKey: requestID) else { continue }
            active.task.cancel()
            finish(
                active.envelope,
                sequenceNumber: 0,
                observation: .cancelled,
                delivery: active.delivery
            )
        }

        let streamIDs = playbackStreams.compactMap { requestID, stream in
            stream.envelope.cancellationId == cancellationID ? requestID : nil
        }
        for requestID in streamIDs {
            guard let stream = playbackStreams.removeValue(forKey: requestID) else { continue }
            finish(
                stream.envelope,
                sequenceNumber: stream.sequenceNumber + 1,
                observation: .cancelled,
                delivery: stream.delivery
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

    private func rememberCompletion(_ requestID: HostRequestId) {
        guard completedRequestIDs.insert(requestID).inserted else { return }
        completionOrder.append(requestID)
        if completionOrder.count > 256 {
            completedRequestIDs.remove(completionOrder.removeFirst())
        }
    }
}
