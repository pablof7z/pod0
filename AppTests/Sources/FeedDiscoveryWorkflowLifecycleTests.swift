import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

final class FeedDiscoveryWorkflowLifecycleTests: XCTestCase {
    private let base = Date(timeIntervalSince1970: 1_800_000_000)

    func testEveryLegacyChildLifecycleStateMapsDeterministically() throws {
        let podcastID = UUID()
        let cases: [(WorkJobState, LegacyFeedDiscoveryDispositionInput)] = [
            (.pending, pending()),
            (.leased, .ambiguous(attempt: 1)),
            (.running, .ambiguous(attempt: 1)),
            (.retryScheduled, pending()),
            (.blocked, pending()),
            (.failedPermanent, .failed(attempt: 1)),
            (.cancelled, .obsolete(attempt: 1)),
            (.obsolete, .obsolete(attempt: 1)),
            (.succeeded, .succeeded(attempt: 1)),
        ]
        let episodes = cases.map { _ in episode(podcastID: podcastID) }
        let jobs = try zip(cases, episodes).map { pair in
            let (item, episode) = pair
            return try notificationChild(
                episode: episode,
                podcastID: podcastID,
                state: item.0,
                attempt: 1,
                notBefore: base.addingTimeInterval(10)
            )
        }

        let result = try LegacyFeedDiscoveryWorkflowMapper.map(
            backup: backup(jobs: jobs),
            state: state(podcastID: podcastID, episodes: episodes),
            now: base.addingTimeInterval(60)
        )

        XCTAssertEqual(result.candidates.count, cases.count)
        for (index, item) in cases.enumerated() {
            let candidate = try XCTUnwrap(result.candidates.first {
                $0.episodeId.uuid == episodes[index].id
            })
            XCTAssertEqual(candidate.disposition, item.1, "\(item.0)")
        }
    }

    func testLiveAndExpiredNotificationLeasesBothPreventRedelivery() throws {
        let podcastID = UUID()
        let episodes = [episode(podcastID: podcastID), episode(podcastID: podcastID)]
        let jobs = try [
            notificationChild(
                episode: episodes[0],
                podcastID: podcastID,
                state: .leased,
                leaseToken: UUID(),
                leaseExpiresAt: base.addingTimeInterval(300)
            ),
            notificationChild(
                episode: episodes[1],
                podcastID: podcastID,
                state: .leased,
                leaseToken: UUID(),
                leaseExpiresAt: base.addingTimeInterval(-300)
            ),
        ]

        let result = try LegacyFeedDiscoveryWorkflowMapper.map(
            backup: backup(jobs: jobs),
            state: state(podcastID: podcastID, episodes: episodes),
            now: base
        )

        XCTAssertEqual(result.candidates.count, 2)
        XCTAssertTrue(result.candidates.allSatisfy {
            $0.disposition == .ambiguous(attempt: 0)
        })
    }

    func testDeliveredArtifactWinsOverPendingNotificationState() throws {
        let podcastID = UUID()
        let episode = episode(podcastID: podcastID)
        let job = try notificationChild(
            episode: episode,
            podcastID: podcastID,
            state: .pending
        )
        let artifact = LegacyFeedDiscoveryArtifactRecord(
            kind: .notificationDelivery,
            subjectID: episode.id,
            inputVersion: job.inputVersion,
            outputVersion: "delivered",
            contentHash: String(repeating: "d", count: 64),
            location: nil,
            origin: "notification",
            schemaVersion: 1,
            integrity: .available,
            verifiedAt: base
        )

        let result = try LegacyFeedDiscoveryWorkflowMapper.map(
            backup: backup(jobs: [job], artifacts: [artifact]),
            state: state(podcastID: podcastID, episodes: [episode]),
            now: base
        )

        XCTAssertEqual(result.candidates.first?.disposition, .succeeded(attempt: 0))
    }

    func testStaleEpisodeInputBecomesObsolete() throws {
        let podcastID = UUID()
        let episode = episode(podcastID: podcastID)
        let job = try notificationChild(
            episode: episode,
            podcastID: podcastID,
            inputVersion: String(repeating: "e", count: 64),
            state: .pending
        )

        let result = try LegacyFeedDiscoveryWorkflowMapper.map(
            backup: backup(jobs: [job]),
            state: state(podcastID: podcastID, episodes: [episode]),
            now: base
        )

        XCTAssertEqual(result.candidates.first?.disposition, .obsolete(attempt: 0))
    }

    func testCorruptAndUnsupportedPayloadVersionsFailClosed() throws {
        let podcastID = UUID()
        let episode = episode(podcastID: podcastID)
        let valid = try notificationChild(
            episode: episode,
            podcastID: podcastID,
            state: .pending
        )
        for version in [0, 2] {
            let unsupported = LegacyFeedDiscoveryWorkflowTestSupport.makeJob(
                kind: valid.kind,
                subjectID: valid.subjectID,
                inputVersion: valid.inputVersion,
                occurrenceID: valid.occurrenceID ?? "notification:unsupported",
                payload: try XCTUnwrap(valid.payload),
                payloadVersion: version
            )
            XCTAssertThrowsError(try LegacyFeedDiscoveryWorkflowMapper.map(
                backup: backup(jobs: [unsupported]),
                state: state(podcastID: podcastID, episodes: [episode]),
                now: base
            )) {
                guard case .futurePayload = $0 as? LegacyFeedDiscoveryWorkflowMappingError
                else { return XCTFail("Expected unsupported payload failure, got \($0)") }
            }
        }
        let corrupt = LegacyFeedDiscoveryWorkflowTestSupport.makeJob(
            kind: valid.kind,
            subjectID: valid.subjectID,
            inputVersion: valid.inputVersion,
            occurrenceID: valid.occurrenceID ?? "notification:corrupt",
            payload: Data("not-json".utf8)
        )
        XCTAssertThrowsError(try LegacyFeedDiscoveryWorkflowMapper.map(
            backup: backup(jobs: [corrupt]),
            state: state(podcastID: podcastID, episodes: [episode]),
            now: base
        )) {
            guard case .corruptJob = $0 as? LegacyFeedDiscoveryWorkflowMappingError
            else { return XCTFail("Expected corruptJob, got \($0)") }
        }
    }

    private func pending() -> LegacyFeedDiscoveryDispositionInput {
        .pending(
            attempt: 1,
            notBefore: UnixTimestampMilliseconds(date: base.addingTimeInterval(10))
        )
    }

    private func notificationChild(
        episode: Episode,
        podcastID: UUID,
        inputVersion: String? = nil,
        state: WorkJobState,
        attempt: Int = 0,
        notBefore: Date = .distantPast,
        leaseToken: UUID? = nil,
        leaseExpiresAt: Date? = nil
    ) throws -> LegacyFeedDiscoveryWorkJob {
        LegacyFeedDiscoveryWorkflowTestSupport.makeJob(
            kind: .newEpisodeNotification,
            subjectID: episode.id,
            inputVersion: inputVersion ?? DesiredStatePlanner.audioVersion(episode),
            occurrenceID: "notification:standalone:\(episode.id.uuidString)",
            payload: try LegacyFeedDiscoveryWorkflowTestSupport.encode(
                LegacyNewEpisodeNotificationPayload(
                    discoveredAt: base,
                    podcastID: podcastID,
                    episodeTitle: episode.title
                )
            ),
            state: state,
            attempt: attempt,
            notBefore: notBefore,
            leaseToken: leaseToken,
            leaseExpiresAt: leaseExpiresAt
        )
    }

    private func backup(
        jobs: [LegacyFeedDiscoveryWorkJob],
        artifacts: [LegacyFeedDiscoveryArtifactRecord] = []
    ) -> LegacyFeedDiscoveryWorkflowBackup {
        LegacyFeedDiscoveryWorkflowBackup(
            formatVersion: 1,
            persistenceGeneration: 7,
            capturedAt: base,
            notificationsEnabled: true,
            jobs: jobs,
            artifacts: artifacts
        )
    }

    private func state(podcastID: UUID, episodes: [Episode]) -> AppState {
        var value = AppState()
        value.podcasts = [Podcast(id: podcastID, title: "Show")]
        value.episodes = episodes
        return value
    }

    private func episode(podcastID: UUID) -> Episode {
        Episode(
            podcastID: podcastID,
            guid: UUID().uuidString,
            title: "Episode",
            pubDate: base,
            enclosureURL: URL(string: "https://example.test/audio.mp3")!
        )
    }
}
