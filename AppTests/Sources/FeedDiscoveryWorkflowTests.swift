import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

final class FeedDiscoveryWorkflowTests: XCTestCase {
    private let base = Date(timeIntervalSince1970: 1_800_000_000)

    func testMapperPlansLatestDownloadsAndNotificationsDeterministically() throws {
        let podcastID = UUID()
        let episodes = (0..<5).map { index in
            Episode(
                podcastID: podcastID,
                guid: "episode-\(index)",
                title: "Episode \(index)",
                pubDate: base.addingTimeInterval(Double(index)),
                enclosureURL: URL(string: "https://example.test/\(index).mp3")!
            )
        }
        let parent = try parentJob(
            podcastID: podcastID,
            episodes: episodes,
            policy: Podcastr.AutoDownloadPolicy(mode: .latestN(2), wifiOnly: false)
        )
        let result = try LegacyFeedDiscoveryWorkflowMapper.map(
            backup: backup(jobs: [parent]),
            state: state(podcastID: podcastID, episodes: episodes),
            now: base.addingTimeInterval(60)
        )

        XCTAssertEqual(result.blockedCount, 0)
        let downloads = result.candidates.filter {
            $0.kind == LegacyFeedDiscoveryEffectKindInput.download
        }
        let notifications = result.candidates.filter {
            $0.kind == LegacyFeedDiscoveryEffectKindInput.notification
        }
        XCTAssertEqual(downloads.count, 2)
        XCTAssertEqual(notifications.count, 3)
        let actualDownloadIDs = Set(downloads.compactMap(\.episodeId.uuid))
        let expectedDownloadIDs = Set(episodes.suffix(2).map(\.id))
        XCTAssertEqual(actualDownloadIDs, expectedDownloadIDs)
        let parentIdentifier = parent.id.uuidString
            .replacingOccurrences(of: "-", with: "")
            .lowercased()
        XCTAssertTrue(result.candidates.allSatisfy {
            $0.sourceOccurrenceId.stableString == parentIdentifier
        })
    }

    func testInterruptedNotificationIsAmbiguousAndNeverReplayedAsPending() throws {
        let podcastID = UUID()
        let episode = episode(podcastID: podcastID)
        let parent = try parentJob(podcastID: podcastID, episodes: [episode], policy: nil)
        let occurrence = "notification:discovery:test:\(episode.id.uuidString)"
        let child = LegacyFeedDiscoveryWorkflowTestSupport.makeJob(
            kind: .newEpisodeNotification,
            subjectID: episode.id,
            inputVersion: DesiredStatePlanner.audioVersion(episode),
            occurrenceID: occurrence,
            payload: try LegacyFeedDiscoveryWorkflowTestSupport.encode(
                LegacyNewEpisodeNotificationPayload(
                    discoveredAt: base,
                    podcastID: podcastID,
                    episodeTitle: episode.title
                )
            ),
            state: .running,
            attempt: 2,
            leaseToken: UUID(),
            externalOperationID: "native-request",
            externalOperationState: "submitted"
        )
        let result = try LegacyFeedDiscoveryWorkflowMapper.map(
            backup: backup(jobs: [parent, child]),
            state: state(podcastID: podcastID, episodes: [episode]),
            now: base.addingTimeInterval(60)
        )

        XCTAssertEqual(result.candidates.count, 1)
        XCTAssertEqual(
            result.candidates[0].kind,
            LegacyFeedDiscoveryEffectKindInput.notification
        )
        XCTAssertEqual(
            result.candidates[0].disposition,
            LegacyFeedDiscoveryDispositionInput.ambiguous(attempt: 2)
        )
    }

    func testMissingEpisodeIsBlockedAndExpiredCandidateIsObsolete() throws {
        let podcastID = UUID()
        let existing = episode(podcastID: podcastID)
        let missing = episode(podcastID: podcastID)
        let parent = try parentJob(
            podcastID: podcastID,
            episodes: [existing, missing],
            policy: Podcastr.AutoDownloadPolicy(mode: .allNew, wifiOnly: false)
        )
        let result = try LegacyFeedDiscoveryWorkflowMapper.map(
            backup: backup(jobs: [parent]),
            state: state(podcastID: podcastID, episodes: [existing]),
            now: base.addingTimeInterval(24 * 60 * 60 + 1)
        )

        XCTAssertEqual(result.blockedCount, 1)
        XCTAssertEqual(result.candidates.count, 2)
        XCTAssertTrue(result.candidates.allSatisfy {
            if case .obsolete = $0.disposition { return true }
            return false
        })
    }

    func testMalformedDuplicateChildFailsClosed() throws {
        let podcastID = UUID()
        let episode = episode(podcastID: podcastID)
        let parent = try parentJob(podcastID: podcastID, episodes: [episode], policy: nil)
        let occurrence = "notification:discovery:test:\(episode.id.uuidString)"
        let payload = try LegacyFeedDiscoveryWorkflowTestSupport.encode(
            LegacyNewEpisodeNotificationPayload(
                discoveredAt: base,
                podcastID: podcastID,
                episodeTitle: episode.title
            )
        )
        let first = LegacyFeedDiscoveryWorkflowTestSupport.makeJob(
            kind: .newEpisodeNotification,
            subjectID: episode.id,
            inputVersion: DesiredStatePlanner.audioVersion(episode),
            occurrenceID: occurrence,
            payload: payload
        )
        let second = LegacyFeedDiscoveryWorkflowTestSupport.makeJob(
            kind: .newEpisodeNotification,
            subjectID: episode.id,
            inputVersion: DesiredStatePlanner.audioVersion(episode),
            occurrenceID: occurrence,
            payload: payload
        )

        XCTAssertThrowsError(try LegacyFeedDiscoveryWorkflowMapper.map(
            backup: backup(jobs: [parent, first, second]),
            state: state(podcastID: podcastID, episodes: [episode]),
            now: base
        )) {
            guard case .duplicateCandidate = $0 as? LegacyFeedDiscoveryWorkflowMappingError
            else { return XCTFail("Expected duplicateCandidate, got \($0)") }
        }
    }

    func testDuplicateEpisodeStateFailsClosedWithoutTrapping() throws {
        let podcastID = UUID()
        let episode = episode(podcastID: podcastID)
        let parent = try parentJob(podcastID: podcastID, episodes: [episode], policy: nil)

        XCTAssertThrowsError(try LegacyFeedDiscoveryWorkflowMapper.map(
            backup: backup(jobs: [parent]),
            state: state(podcastID: podcastID, episodes: [episode, episode]),
            now: base
        )) {
            XCTAssertEqual(
                $0 as? LegacyFeedDiscoveryWorkflowMappingError,
                .duplicateEpisode(episode.id)
            )
        }
    }

    func testDuplicateParentOccurrenceFailsClosed() throws {
        let podcastID = UUID()
        let episode = episode(podcastID: podcastID)
        let first = try parentJob(podcastID: podcastID, episodes: [episode], policy: nil)
        let second = try parentJob(podcastID: podcastID, episodes: [episode], policy: nil)

        XCTAssertThrowsError(try LegacyFeedDiscoveryWorkflowMapper.map(
            backup: backup(jobs: [first, second]),
            state: state(podcastID: podcastID, episodes: [episode]),
            now: base
        )) {
            guard case .duplicateOccurrence = $0 as? LegacyFeedDiscoveryWorkflowMappingError
            else { return XCTFail("Expected duplicateOccurrence, got \($0)") }
        }
    }

    private func parentJob(
        podcastID: UUID,
        episodes: [Episode],
        policy: Podcastr.AutoDownloadPolicy?
    ) throws -> LegacyFeedDiscoveryWorkJob {
        let occurrence = "discovery:test"
        let payload = LegacyFeedDiscoveryPayload(
            podcastID: podcastID,
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
            autoDownloadPolicy: policy,
            notificationsEnabled: true,
            policyVersion: "feed-policy-v1"
        )
        return LegacyFeedDiscoveryWorkflowTestSupport.makeJob(
            kind: .feedDiscovery,
            subjectID: podcastID,
            inputVersion: String(repeating: "f", count: 64),
            occurrenceID: occurrence,
            payload: try LegacyFeedDiscoveryWorkflowTestSupport.encode(payload)
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
