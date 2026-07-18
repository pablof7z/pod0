import Foundation
import Pod0Core
@testable import Podcastr
import XCTest

final class LegacyListeningImportBindingTests: XCTestCase {
    func testCurrentSwiftSQLiteStagesAndVerifiesThroughGeneratedFacade() throws {
        let root = URL(fileURLWithPath: NSTemporaryDirectory(), isDirectory: true)
            .appendingPathComponent("pod0-listening-import-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let stateURL = root.appendingPathComponent("podcastr-state.v1.json")
        let persistence = Persistence(fileURL: stateURL)
        defer {
            persistence.reset()
            try? FileManager.default.removeItem(at: root)
        }

        let podcastUUID = try XCTUnwrap(UUID(uuidString: "11111111-1111-1111-1111-111111111111"))
        let episodeUUID = try XCTUnwrap(UUID(uuidString: "22222222-2222-2222-2222-222222222222"))
        var state = AppState()
        state.podcasts = [Podcast(
            id: podcastUUID,
            kind: .rss,
            feedURL: URL(string: "https://EXAMPLE.test/Feed")!,
            title: "Imported show",
            author: "Pod0",
            imageURL: URL(string: "https://example.test/show.png"),
            description: "A living knowledge base",
            language: "en",
            categories: ["Technology"],
            discoveredAt: Date(timeIntervalSince1970: 1_704_067_200),
            lastRefreshedAt: Date(timeIntervalSince1970: 1_704_153_600),
            etag: "etag-v1",
            lastModified: "yesterday"
        )]
        state.subscriptions = [PodcastSubscription(
            podcastID: podcastUUID,
            subscribedAt: Date(timeIntervalSince1970: 1_704_240_000),
            autoDownload: .init(mode: .latestN(3), wifiOnly: false),
            notificationsEnabled: false,
            defaultPlaybackRate: 1.5
        )]
        state.episodes = [Episode(
            id: episodeUUID,
            podcastID: podcastUUID,
            guid: "publisher-guid",
            title: "Imported episode",
            description: "Preserve this context",
            pubDate: Date(timeIntervalSince1970: 1_704_067_200),
            duration: 120.5,
            enclosureURL: URL(string: "https://example.test/episode.mp3")!,
            enclosureMimeType: "audio/mpeg",
            playbackPosition: 32.25,
            played: true,
            isStarred: true,
            downloadState: .downloaded(
                localFileURL: root.appendingPathComponent("episode.mp3"),
                byteCount: 4_096
            ),
            transcriptState: .ready(source: .publisher)
        )]
        state.lastPlayedEpisodeID = episodeUUID
        state.settings.defaultPlaybackRate = 1.25
        state.settings.autoMarkPlayedAtEnd = false
        state.settings.autoPlayNext = false
        XCTAssertEqual(persistence.save(state), 1)

        let source = persistence.episodeStore.fileURL.path
        let sourceBackup = root.appendingPathComponent("swift.backup.sqlite").path
        let target = root.appendingPathComponent("core.sqlite").path
        let targetBackup = root.appendingPathComponent("core.backup.sqlite").path
        let importID = CommandId(high: 0, low: 1)
        let plan = try inspectLegacyListeningSource(sourcePath: source)
        let report = try stageLegacyListeningImport(
            sourcePath: source,
            sourceBackupPath: sourceBackup,
            targetPath: target,
            targetSchemaBackupPath: targetBackup,
            expectedPlan: plan,
            importId: importID,
            targetStoreId: CommandId(high: 0, low: 2),
            observedAtMilliseconds: 1_721_322_000_000
        )
        XCTAssertTrue(report.staged)
        XCTAssertFalse(report.reusedExisting)

        let verification = try readStagedLegacyListeningImport(
            targetPath: target,
            importId: importID
        )
        XCTAssertEqual(verification.snapshot.podcasts.count, 1)
        XCTAssertEqual(
            verification.snapshot.podcasts[0].podcastId,
            PodcastId(high: 0x1111_1111_1111_1111, low: 0x1111_1111_1111_1111)
        )
        let episode = try XCTUnwrap(verification.snapshot.episodes.first)
        XCTAssertEqual(episode.episodeId, EpisodeId(
            high: 0x2222_2222_2222_2222,
            low: 0x2222_2222_2222_2222
        ))
        XCTAssertEqual(episode.listening.resumePositionMilliseconds, 32_250)
        XCTAssertEqual(episode.listening.completion, .completed(cause: .legacyPlayedFlag))
        XCTAssertTrue(episode.isStarred)
        XCTAssertEqual(verification.snapshot.playback.activeEpisodeId, episode.episodeId)
        XCTAssertEqual(verification.snapshot.playback.rate.value, 1_250)

        let sourceAfterImport = try persistence.load()
        XCTAssertEqual(sourceAfterImport.episodes.first?.id, episodeUUID)
        XCTAssertEqual(sourceAfterImport.episodes.first?.playbackPosition, 32.25)
        XCTAssertEqual(sourceAfterImport.lastPlayedEpisodeID, episodeUUID)
    }
}
