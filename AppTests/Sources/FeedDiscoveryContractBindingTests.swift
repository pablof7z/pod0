import Pod0Core
import XCTest

final class FeedDiscoveryContractBindingTests: XCTestCase {
    func testNotificationPolicyAndCapabilityBoundaryRemainTyped() {
        let occurrenceID = FeedDiscoveryOccurrenceId(high: 1, low: 2)
        let episodeID = EpisodeId(high: 3, low: 4)
        let podcastID = PodcastId(high: 5, low: 6)

        let command = ApplicationCommand.setNewEpisodeNotificationsEnabled(enabled: false)
        guard case let .setNewEpisodeNotificationsEnabled(enabled) = command else {
            return XCTFail("Expected a typed notification setting command")
        }
        XCTAssertFalse(enabled)

        let settings = NewEpisodeNotificationSettingsProjection(
            enabled: true,
            revision: StateRevision(value: 7)
        )
        XCTAssertEqual(
            Projection.newEpisodeNotificationSettings(value: settings),
            .newEpisodeNotificationSettings(value: settings)
        )
        XCTAssertEqual(
            ProjectionScope.newEpisodeNotificationSettings,
            .newEpisodeNotificationSettings
        )

        let request = HostRequest.deliverNewEpisodeNotification(
            occurrenceId: occurrenceID,
            episodeId: episodeID,
            podcastId: podcastID,
            podcastTitle: "Podcast",
            episodeTitle: "Episode"
        )
        guard case let .deliverNewEpisodeNotification(
            requestOccurrenceID,
            requestEpisodeID,
            requestPodcastID,
            podcastTitle,
            episodeTitle
        ) = request else {
            return XCTFail("Expected a typed notification request")
        }
        XCTAssertEqual(requestOccurrenceID, occurrenceID)
        XCTAssertEqual(requestEpisodeID, episodeID)
        XCTAssertEqual(requestPodcastID, podcastID)
        XCTAssertEqual(podcastTitle, "Podcast")
        XCTAssertEqual(episodeTitle, "Episode")

        XCTAssertEqual(
            HostObservation.newEpisodeNotificationDelivered(
                occurrenceId: occurrenceID,
                episodeId: episodeID
            ),
            .newEpisodeNotificationDelivered(
                occurrenceId: occurrenceID,
                episodeId: episodeID
            )
        )
        XCTAssertEqual(
            CoreWakeReason.feedDiscoveryNotificationRetry(
                occurrenceId: occurrenceID,
                episodeId: episodeID,
                attempt: 2
            ),
            .feedDiscoveryNotificationRetry(
                occurrenceId: occurrenceID,
                episodeId: episodeID,
                attempt: 2
            )
        )
    }

    func testNotificationSettingsProjectionIsAvailableThroughTheFacade() {
        let facade = Pod0Facade()
        let envelope = facade.snapshot(
            request: ProjectionRequest(
                scope: .newEpisodeNotificationSettings,
                offset: 0,
                maxItems: 1
            )
        )

        XCTAssertEqual(envelope.contractVersion, 46)
        guard case let .newEpisodeNotificationSettings(value) = envelope.projection else {
            return XCTFail("Expected notification settings projection")
        }
        XCTAssertTrue(value.enabled)
        XCTAssertEqual(value.revision, StateRevision(value: 0))
    }
}
