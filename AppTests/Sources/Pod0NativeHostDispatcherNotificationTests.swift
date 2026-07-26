import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class Pod0NativeHostDispatcherNotificationTests: XCTestCase {
    func testDispatcherStartsCancelsAndShutsDownNotificationCapability() async {
        let notifications = SuspendingNotificationHost()
        let dispatcher = Pod0NativeHostDispatcher(
            feedHost: NotificationFeedHost(),
            notificationHost: notifications,
            playbackHost: NotificationPlaybackHost()
        )
        let occurrenceID = FeedDiscoveryOccurrenceId(high: 10, low: 11)
        let request = HostRequest.deliverNewEpisodeNotification(
            occurrenceId: occurrenceID,
            episodeId: EpisodeId(high: 12, low: 13),
            podcastId: PodcastId(high: 14, low: 15),
            podcastTitle: "Podcast",
            episodeTitle: "Episode"
        )
        let envelope = HostRequestEnvelope(
            requestId: HostRequestId(high: 20, low: 21),
            commandId: CommandId(high: 22, low: 23),
            cancellationId: CancellationId(high: 24, low: 25),
            issuedRevision: StateRevision(value: 26),
            deadlineAt: nil,
            request: request
        )
        var observations: [HostObservationEnvelope] = []

        dispatcher.execute(envelope) { observations.append($0) }
        while notifications.deliveryCount == 0 {
            await Task.yield()
        }
        dispatcher.cancel(
            requestID: envelope.requestId,
            cancellationID: envelope.cancellationId
        )
        await Task.yield()

        XCTAssertEqual(notifications.cancelledOccurrenceIDs, [occurrenceID])
        XCTAssertTrue(observations.isEmpty)
        XCTAssertTrue(dispatcher.activeTasks.isEmpty)

        dispatcher.shutdown()
        XCTAssertEqual(notifications.shutdownCount, 1)
    }
}

@MainActor
private final class SuspendingNotificationHost: CoreNotificationHosting {
    var deliveryCount = 0
    var cancelledOccurrenceIDs: [FeedDiscoveryOccurrenceId] = []
    var shutdownCount = 0

    func deliver(
        occurrenceID _: FeedDiscoveryOccurrenceId,
        episodeID _: EpisodeId,
        podcastID _: PodcastId,
        podcastTitle _: String,
        episodeTitle _: String
    ) async -> HostObservation {
        deliveryCount += 1
        do {
            try await Task.sleep(for: .seconds(30))
            return .failed(code: .platformFailure, safeDetail: "Unexpected completion")
        } catch {
            return .cancelled
        }
    }

    func cancel(occurrenceID: FeedDiscoveryOccurrenceId) {
        cancelledOccurrenceIDs.append(occurrenceID)
    }

    func shutdown() {
        shutdownCount += 1
    }
}

private actor NotificationFeedHost: CoreFeedHosting {
    func fetch(
        feedURL _: String,
        entityTag _: String?,
        lastModified _: String?,
        maximumResponseBytes _: UInt64,
        deadline _: Date?
    ) async -> HostObservation {
        .failed(code: .platformFailure, safeDetail: "Unexpected feed request")
    }
}

@MainActor
private final class NotificationPlaybackHost: CorePlaybackHosting {
    func execute(_: HostRequest) -> HostObservation {
        .failed(code: .platformFailure, safeDetail: "Unexpected playback request")
    }

    func installObservationSink(_: @escaping (PlaybackLifecycleObservation) -> Void) {}
}
