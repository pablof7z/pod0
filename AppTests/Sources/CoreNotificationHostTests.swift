import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class CoreNotificationHostTests: XCTestCase {
    func testDeliversExactTypedNotificationAndTapRoute() async {
        let center = RecordingNotificationCenter()
        let host = CoreNotificationHost(center: center)
        let occurrenceID = FeedDiscoveryOccurrenceId(high: 1, low: 2)
        let episodeUUID = try! XCTUnwrap(UUID(
            uuidString: "00000000-0000-0003-0000-000000000004"
        ))
        let podcastUUID = try! XCTUnwrap(UUID(
            uuidString: "00000000-0000-0005-0000-000000000006"
        ))

        let observation = await host.deliver(
            occurrenceID: occurrenceID,
            episodeID: EpisodeId(uuid: episodeUUID),
            podcastID: PodcastId(uuid: podcastUUID),
            podcastTitle: "Exact podcast",
            episodeTitle: "Exact episode"
        )

        XCTAssertEqual(
            observation,
            .newEpisodeNotificationDelivered(
                occurrenceId: occurrenceID,
                episodeId: EpisodeId(uuid: episodeUUID)
            )
        )
        XCTAssertEqual(center.requests, [CoreNotificationRequest(
            identifier: "00000000000000010000000000000002",
            title: "Exact podcast",
            body: "New episode: Exact episode",
            threadIdentifier: "podcast:\(podcastUUID.uuidString)",
            episodeID: episodeUUID.uuidString,
            occurrenceID: "00000000000000010000000000000002"
        )])
        XCTAssertEqual(
            AppDelegate.notificationDeepLink(userInfo: [
                NotificationService.episodeIDUserInfoKey: episodeUUID.uuidString,
            ]),
            URL(string: "podcastr://episode/\(episodeUUID.uuidString)")
        )
    }

    func testRepeatedDeliveryUsesStableSystemIdentifierOnlyOnce() async {
        let center = RecordingNotificationCenter()
        let host = CoreNotificationHost(center: center)

        let first = await deliver(host)
        let second = await deliver(host)

        XCTAssertEqual(first, second)
        XCTAssertEqual(center.requests.count, 1)
    }

    func testPermissionDenialReturnsRawTypedFailureWithoutScheduling() async {
        let center = RecordingNotificationCenter()
        center.authorizationValue = .denied
        let host = CoreNotificationHost(center: center)

        let observation = await deliver(host)

        guard case .failed(code: .permissionDenied, safeDetail: nil) = observation else {
            return XCTFail("Expected permission-denied observation")
        }
        XCTAssertTrue(center.requests.isEmpty)
    }

    func testUndeterminedPermissionRequestsAuthorizationOnce() async {
        let center = RecordingNotificationCenter()
        center.authorizationValue = .notDetermined
        let host = CoreNotificationHost(center: center)

        let observation = await deliver(host)

        guard case .newEpisodeNotificationDelivered = observation else {
            return XCTFail("Expected delivered observation")
        }
        XCTAssertEqual(center.authorizationRequests, 1)
        XCTAssertEqual(center.requests.count, 1)
    }

    func testPlatformFailureReturnsRawFailureWithoutNativeRetry() async {
        let center = RecordingNotificationCenter()
        center.addError = NotificationTestError.delivery
        let host = CoreNotificationHost(center: center)

        let observation = await deliver(host)

        guard case .failed(code: .platformFailure, safeDetail: _) = observation else {
            return XCTFail("Expected platform-failure observation")
        }
        XCTAssertEqual(center.addAttempts, 1)
    }

    func testCancellationRemovesExactRequestAndSuppressesDelivery() async {
        let center = RecordingNotificationCenter()
        center.authorizationValue = .notDetermined
        center.suspendAuthorization = true
        let host = CoreNotificationHost(center: center)
        let occurrenceID = FeedDiscoveryOccurrenceId(high: 1, low: 2)
        let task = Task { @MainActor in await deliver(host) }
        while center.authorizationRequests == 0 {
            await Task.yield()
        }

        host.cancel(occurrenceID: occurrenceID)
        center.resumeAuthorization(granted: true)
        let observation = await task.value

        XCTAssertEqual(observation, .cancelled)
        XCTAssertEqual(center.removedIdentifiers, [
            "00000000000000010000000000000002",
            "00000000000000010000000000000002",
        ])
        XCTAssertTrue(center.requests.isEmpty)
    }

    private func deliver(_ host: CoreNotificationHost) async -> HostObservation {
        await host.deliver(
            occurrenceID: FeedDiscoveryOccurrenceId(high: 1, low: 2),
            episodeID: EpisodeId(high: 3, low: 4),
            podcastID: PodcastId(high: 5, low: 6),
            podcastTitle: "Podcast",
            episodeTitle: "Episode"
        )
    }
}

private enum NotificationTestError: Error {
    case delivery
}

@MainActor
private final class RecordingNotificationCenter: CoreNotificationCentering {
    var authorizationValue = CoreNotificationAuthorization.authorized
    var suspendAuthorization = false
    var addError: Error?
    var requests: [CoreNotificationRequest] = []
    var addAttempts = 0
    var authorizationRequests = 0
    var removedIdentifiers: [String] = []
    private var identifiers: Set<String> = []
    private var authorizationContinuation: CheckedContinuation<Bool, Error>?

    func authorization() async -> CoreNotificationAuthorization {
        authorizationValue
    }

    func requestAuthorization() async throws -> Bool {
        authorizationRequests += 1
        guard suspendAuthorization else { return true }
        return try await withCheckedThrowingContinuation {
            authorizationContinuation = $0
        }
    }

    func existingRequestIdentifiers() async -> Set<String> {
        identifiers
    }

    func add(_ request: CoreNotificationRequest) async throws {
        addAttempts += 1
        if let addError { throw addError }
        requests.append(request)
        identifiers.insert(request.identifier)
    }

    func remove(identifier: String) {
        removedIdentifiers.append(identifier)
        identifiers.remove(identifier)
    }

    func resumeAuthorization(granted: Bool) {
        authorizationContinuation?.resume(returning: granted)
        authorizationContinuation = nil
    }
}
