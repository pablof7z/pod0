import XCTest
@testable import Podcastr

@MainActor
final class AutoDownloadHandoffTests: XCTestCase {
    func testOccurrenceDoesNotSucceedWhenChildIntentPersistenceFails() async throws {
        let made = AppStateTestSupport.makeIsolatedStore()
        defer { AppStateTestSupport.disposeIsolatedStore(at: made.fileURL) }
        let podcast = Podcast(id: UUID(), title: "Automatic")
        let episode = Episode(
            podcastID: podcast.id,
            guid: "automatic",
            title: "Automatic",
            pubDate: Date(),
            enclosureURL: URL(string: "https://example.com/automatic.mp3")!
        )
        made.store.installPodcastFixture(podcast)
        made.store.installSubscriptionFixture(PodcastSubscription(
            podcastID: podcast.id,
            autoDownload: AutoDownloadPolicy(mode: .allNew, wifiOnly: false)
        ))
        made.store.installEpisodeFixtures([episode], forPodcast: podcast.id)
        EpisodeDownloadService.shared.pathState.set(.wifi)
        let context = JobAttemptContext(
            job: workJob(for: episode),
            leaseToken: UUID(),
            deadline: nil
        )
        let expected = JobFailure(
            classification: .unexpected,
            message: "injected persistence failure"
        )
        let executor = AutoDownloadJobExecutor(store: made.store) { _, _ in
            throw expected
        }

        do {
            _ = try await executor.run(context)
            XCTFail("Parent occurrence must not succeed without its durable child")
        } catch let failure as JobFailure {
            XCTAssertEqual(failure, expected)
        }
    }

    private func workJob(for episode: Episode) -> WorkJob {
        WorkJob(
            id: UUID(),
            idempotencyKey: "autodownload:test:\(episode.id)",
            kind: .autoDownload,
            subjectID: episode.id,
            inputVersion: DesiredStatePlanner.audioVersion(episode),
            occurrenceID: "autodownload:test:\(episode.id)",
            payloadVersion: 1,
            payload: nil,
            state: .running,
            priority: 20,
            resourceClass: .planning,
            attempt: 1,
            maxAttempts: 8,
            notBefore: Date(),
            leaseToken: nil,
            leaseOwner: nil,
            leaseExpiresAt: nil,
            externalProvider: nil,
            externalOperationID: nil,
            externalOperationState: nil,
            outputVersion: nil,
            lastErrorClass: nil,
            lastErrorMessage: nil,
            createdAt: Date(),
            updatedAt: Date()
        )
    }
}
