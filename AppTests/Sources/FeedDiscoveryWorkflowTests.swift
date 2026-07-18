import Foundation
import XCTest
@testable import Podcastr

@MainActor
final class FeedDiscoveryWorkflowTests: XCTestCase {
    func testExactDiscoveryBatchPlansLatestNAndNotificationsIdempotently() async throws {
        let made = AppStateTestSupport.makeIsolatedStore()
        defer { AppStateTestSupport.disposeIsolatedStore(at: made.fileURL) }
        let store = made.store
        let jobs = JobStore(fileURL: store.persistence.episodeStore.fileURL)
        let podcast = Podcast(
            id: UUID(),
            feedURL: URL(string: "https://example.com/feed.xml"),
            title: "Discovery"
        )
        let base = Date(timeIntervalSince1970: 20_000)
        let episodes = (0..<5).map { index in
            Episode(
                podcastID: podcast.id,
                guid: "episode-\(index)",
                title: "Episode \(index)",
                pubDate: base.addingTimeInterval(Double(index)),
                enclosureURL: URL(string: "https://example.com/\(index).mp3")!
            )
        }
        store.mutateState {
            $0.podcasts = [podcast]
            $0.subscriptions = [PodcastSubscription(
                podcastID: podcast.id,
                autoDownload: AutoDownloadPolicy(mode: .latestN(2), wifiOnly: false),
                notificationsEnabled: true
            )]
            $0.episodes = episodes
            $0.settings.notifyOnNewEpisodes = true
        }
        let occurrence = "discovery:test-batch"
        let payload = FeedDiscoveryPayload(
            podcastID: podcast.id,
            occurrenceID: occurrence,
            discoveredAt: base,
            episodes: episodes.map {
                .init(
                    episodeID: $0.id,
                    inputVersion: DesiredStatePlanner.audioVersion($0),
                    pubDate: $0.pubDate,
                    title: $0.title
                )
            },
            autoDownloadPolicy: AutoDownloadPolicy(mode: .latestN(2), wifiOnly: false),
            notificationsEnabled: true,
            policyVersion: "feed-policy-v1"
        )
        let desired = DesiredJob(
            idempotencyKey: occurrence,
            kind: .feedDiscovery,
            subjectID: podcast.id,
            inputVersion: "batch-v1",
            occurrenceID: occurrence,
            payload: try workflowData(payload),
            resourceClass: .planning
        )
        _ = try jobs.ensureJob(desired)
        let claimed = try XCTUnwrap(try jobs.claimDueJobs(
            resourceClass: .planning,
            capacity: 1,
            now: Date(),
            owner: "feed-test",
            leaseDuration: 60
        ).first)
        let context = JobAttemptContext(
            job: claimed,
            leaseToken: try XCTUnwrap(claimed.leaseToken),
            deadline: claimed.leaseExpiresAt
        )
        let executor = FeedDiscoveryJobExecutor(store: store, jobStore: jobs)

        _ = try await executor.run(context)
        _ = try await executor.run(context)

        let created = try jobs.allJobs()
        let automatic = created.filter { $0.kind == .autoDownload }
        let notifications = created.filter { $0.kind == .newEpisodeNotification }
        XCTAssertEqual(automatic.count, 2)
        XCTAssertEqual(
            Set(automatic.map(\.subjectID)),
            Set(episodes.sorted { $0.pubDate > $1.pubDate }.prefix(2).map(\.id))
        )
        XCTAssertEqual(notifications.count, 3)
        XCTAssertEqual(Set(notifications.compactMap(\.occurrenceID)).count, 3)
        XCTAssertEqual(
            Set(notifications.map(\.subjectID)),
            Set(episodes.sorted { $0.pubDate > $1.pubDate }.prefix(3).map(\.id))
        )
    }

    func testExpiredNotificationOccurrenceBecomesObsoleteBeforeDelivery() async throws {
        let made = AppStateTestSupport.makeIsolatedStore()
        defer { AppStateTestSupport.disposeIsolatedStore(at: made.fileURL) }
        let podcast = Podcast(id: UUID(), title: "Old")
        let episode = Episode(
            podcastID: podcast.id, guid: "old", title: "Old episode",
            pubDate: Date(), enclosureURL: URL(string: "https://example.com/old.mp3")!
        )
        made.store.mutateState {
            $0.podcasts = [podcast]
            $0.subscriptions = [PodcastSubscription(
                podcastID: podcast.id,
                notificationsEnabled: true
            )]
            $0.episodes = [episode]
        }
        let occurrence = "notification:old"
        let payload = NotificationJobPayload(
            discoveredAt: Date().addingTimeInterval(-(24 * 60 * 60 + 1)),
            podcastID: podcast.id,
            episodeTitle: episode.title
        )
        let job = workJob(
            kind: .newEpisodeNotification,
            subjectID: episode.id,
            occurrenceID: occurrence,
            payload: try workflowData(payload)
        )

        let outcome = try await NewEpisodeNotificationJobExecutor(
            store: made.store
        ).run(JobAttemptContext(job: job, leaseToken: UUID(), deadline: nil))

        XCTAssertEqual(outcome, .obsolete)
        XCTAssertEqual(
            NotificationService.requestIdentifier(
                episodeID: episode.id,
                occurrenceID: occurrence
            ),
            occurrence
        )
    }

    func testGlobalNotificationTogglePreventsDiscoveryFromCreatingDeliveryJobs() async throws {
        let made = AppStateTestSupport.makeIsolatedStore()
        defer { AppStateTestSupport.disposeIsolatedStore(at: made.fileURL) }
        let podcast = Podcast(id: UUID(), title: "Muted")
        let episode = Episode(
            podcastID: podcast.id, guid: "muted", title: "Muted episode",
            pubDate: Date(), enclosureURL: URL(string: "https://example.com/muted.mp3")!
        )
        made.store.mutateState {
            $0.podcasts = [podcast]
            $0.subscriptions = [PodcastSubscription(
                podcastID: podcast.id,
                notificationsEnabled: true
            )]
            $0.episodes = [episode]
            $0.settings.notifyOnNewEpisodes = false
        }
        let jobs = JobStore(fileURL: made.store.persistence.episodeStore.fileURL)
        let occurrence = "discovery:muted"
        let payload = FeedDiscoveryPayload(
            podcastID: podcast.id,
            occurrenceID: occurrence,
            discoveredAt: Date(),
            episodes: [.init(
                episodeID: episode.id,
                inputVersion: DesiredStatePlanner.audioVersion(episode),
                pubDate: episode.pubDate,
                title: episode.title
            )],
            autoDownloadPolicy: nil,
            notificationsEnabled: true,
            policyVersion: "feed-policy-v1"
        )
        let job = workJob(
            kind: .feedDiscovery,
            subjectID: podcast.id,
            occurrenceID: occurrence,
            payload: try workflowData(payload)
        )

        _ = try await FeedDiscoveryJobExecutor(
            store: made.store,
            jobStore: jobs
        ).run(JobAttemptContext(job: job, leaseToken: UUID(), deadline: nil))

        XCTAssertTrue(try jobs.allJobs().filter {
            $0.kind == .newEpisodeNotification
        }.isEmpty)
    }

    func testGlobalNotificationTogglePreventsFreshDeliveryAtExecutorBoundary() async throws {
        let made = AppStateTestSupport.makeIsolatedStore()
        defer { AppStateTestSupport.disposeIsolatedStore(at: made.fileURL) }
        let podcast = Podcast(id: UUID(), title: "Muted")
        let episode = Episode(
            podcastID: podcast.id, guid: "fresh-muted", title: "Fresh muted episode",
            pubDate: Date(), enclosureURL: URL(string: "https://example.com/fresh.mp3")!
        )
        made.store.mutateState {
            $0.podcasts = [podcast]
            $0.subscriptions = [PodcastSubscription(
                podcastID: podcast.id,
                notificationsEnabled: true
            )]
            $0.episodes = [episode]
            $0.settings.notifyOnNewEpisodes = false
        }
        let occurrence = "notification:fresh-muted"
        let payload = NotificationJobPayload(
            discoveredAt: Date(),
            podcastID: podcast.id,
            episodeTitle: episode.title
        )
        let job = workJob(
            kind: .newEpisodeNotification,
            subjectID: episode.id,
            occurrenceID: occurrence,
            payload: try workflowData(payload)
        )

        let outcome = try await NewEpisodeNotificationJobExecutor(
            store: made.store
        ).run(JobAttemptContext(job: job, leaseToken: UUID(), deadline: nil))

        XCTAssertEqual(outcome, .obsolete)
    }

    private func workflowData<T: Encodable>(_ value: T) throws -> Data {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.sortedKeys]
        return try encoder.encode(value)
    }

    private func workJob(
        kind: WorkJobKind,
        subjectID: UUID,
        occurrenceID: String,
        payload: Data
    ) -> WorkJob {
        WorkJob(
            id: UUID(), idempotencyKey: occurrenceID, kind: kind,
            subjectID: subjectID, inputVersion: "v1", occurrenceID: occurrenceID,
            payloadVersion: 1, payload: payload, state: .running, priority: 0,
            resourceClass: .notification, attempt: 1, maxAttempts: 4,
            notBefore: Date(), leaseToken: nil, leaseOwner: nil,
            leaseExpiresAt: nil, externalProvider: nil,
            externalOperationID: nil, externalOperationState: nil,
            outputVersion: nil, lastErrorClass: nil, lastErrorMessage: nil,
            createdAt: Date(), updatedAt: Date()
        )
    }
}
