import Foundation
import Pod0Core

extension Pod0NativeHostDispatcher {
    func startPlaybackStream(
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

    func receivePlaybackObservation(_ observation: PlaybackLifecycleObservation) {
        let timestamp = now()
        for requestID in Array(playbackStreams.keys) {
            guard var stream = playbackStreams[requestID],
                  stream.episodeID == nil || stream.episodeID == observation.episodeId
            else { continue }
            let isBoundary = Self.isPlaybackBoundary(
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

    private static func isPlaybackBoundary(
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
}
