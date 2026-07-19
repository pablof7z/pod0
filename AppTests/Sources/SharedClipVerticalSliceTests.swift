import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class SharedClipVerticalSliceTests: XCTestCase {
    func testClipBackupPathsAreGenerationAndContentQualified() {
        let persistence = Persistence(fileURL: AppStateTestSupport.uniqueTempFileURL())
        defer { persistence.reset() }
        let first = LegacyClipImportPlan(
            sourceKind: .legacyJson,
            sourceHash: String(repeating: "a", count: 64),
            sourceGeneration: 7,
            clipCount: 1
        )
        let nextGeneration = LegacyClipImportPlan(
            sourceKind: .legacyJson,
            sourceHash: first.sourceHash,
            sourceGeneration: 8,
            clipCount: 1
        )
        let changedContent = LegacyClipImportPlan(
            sourceKind: .legacyJson,
            sourceHash: String(repeating: "b", count: 64),
            sourceGeneration: 7,
            clipCount: 1
        )
        XCTAssertNotEqual(
            persistence.legacyClipsBackupURL(for: first),
            persistence.legacyClipsBackupURL(for: nextGeneration)
        )
        XCTAssertNotEqual(
            persistence.legacyClipsBackupURL(for: first),
            persistence.legacyClipsBackupURL(for: changedContent)
        )
    }

    func testClipImportFailureCannotReopenCommittedNotePersistence() throws {
        let fileURL = AppStateTestSupport.uniqueTempFileURL()
        let persistence = Persistence(fileURL: fileURL)
        defer { persistence.reset() }
        let podcastID = UUID(uuidString: "11111111-1111-1111-1111-111111111111")!
        let episodeID = UUID(uuidString: "22222222-2222-2222-2222-222222222222")!
        var legacy = AppState()
        legacy.podcasts = [Podcast(
            id: podcastID,
            feedURL: URL(string: "https://cutover.example/feed.xml")!,
            title: "Cutover Order"
        )]
        legacy.subscriptions = [PodcastSubscription(podcastID: podcastID)]
        legacy.episodes = [Episode(
            id: episodeID,
            podcastID: podcastID,
            guid: "cutover-order",
            title: "Cutover Order",
            pubDate: Date(timeIntervalSince1970: 1_700_000_100),
            enclosureURL: URL(string: "https://cutover.example/episode.mp3")!
        )]
        legacy.notes = [Note(text: "Already committed note")]
        legacy.clips = [Clip(
            episodeID: episodeID,
            subscriptionID: podcastID,
            startMs: 10,
            endMs: 10
        )]
        XCTAssertTrue(persistence.write(legacy, revision: 7))

        let store = AppStateStore(
            persistence: persistence,
            sharedFeedHost: QueuedCoreFeedHost([]),
            startSubscriptionRefresh: false
        )
        XCTAssertEqual(store.sharedLibraryUnavailableReason, "SourceInvalid")
        XCTAssertTrue(
            persistence.metadataState(from: store.state).notes.isEmpty,
            "The note cutover must lock Swift persistence before clip inspection fails"
        )
        XCTAssertTrue(persistence.write(store.state, revision: .max))
        let metadata = try XCTUnwrap(persistence.episodeStore.loadMetadata())
        let object = try XCTUnwrap(
            JSONSerialization.jsonObject(with: metadata) as? [String: Any]
        )
        XCTAssertEqual((object["notes"] as? [Any])?.count, 0)
        let rewritten = try persistence.load()
        XCTAssertTrue(rewritten.notes.isEmpty, "Committed notes must never regain a Swift writer")
        XCTAssertEqual(rewritten.clips.count, 1, "Failed clip import remains rollback evidence")
    }

    func testLegacyClipsCutOverLosslesslyAndCommandsSurviveRelaunch() throws {
        let fileURL = AppStateTestSupport.uniqueTempFileURL()
        let persistence = Persistence(fileURL: fileURL)
        defer { persistence.reset() }
        let podcastID = UUID(uuidString: "11111111-1111-1111-1111-111111111111")!
        let episodeID = UUID(uuidString: "22222222-2222-2222-2222-222222222222")!
        let firstID = UUID(uuidString: "33333333-3333-3333-3333-333333333333")!
        let secondID = UUID(uuidString: "44444444-4444-4444-4444-444444444444")!
        let speakerID = UUID(uuidString: "55555555-5555-5555-5555-555555555555")!
        var legacy = AppState()
        legacy.podcasts = [Podcast(
            id: podcastID,
            feedURL: URL(string: "https://clips.example/feed.xml")!,
            title: "Clip Targets",
            discoveredAt: Date(timeIntervalSince1970: 1_700_000_000)
        )]
        legacy.subscriptions = [PodcastSubscription(podcastID: podcastID)]
        legacy.episodes = [Episode(
            id: episodeID,
            podcastID: podcastID,
            guid: "clips-episode",
            title: "Anchored Clips",
            pubDate: Date(timeIntervalSince1970: 1_700_000_100),
            enclosureURL: URL(string: "https://clips.example/episode.mp3")!
        )]
        legacy.clips = [
            Clip(
                id: firstID,
                episodeID: episodeID,
                subscriptionID: podcastID,
                startMs: 12_345,
                endMs: 15_678,
                createdAt: Date(timeIntervalSince1970: 1_704_153_600.125),
                caption: "Legacy exact moment",
                speakerID: "Speaker One",
                transcriptText: "The exact frozen transcript words.",
                source: .touch
            ),
            Clip(
                id: secondID,
                episodeID: episodeID,
                subscriptionID: podcastID,
                startMs: 30_000,
                endMs: 34_000,
                createdAt: Date(timeIntervalSince1970: 1_704_240_000),
                transcriptText: "Deleted but recoverable.",
                source: .carplay,
                deleted: true
            )
        ]
        XCTAssertTrue(persistence.write(legacy, revision: 7))
        let persistedCreatedAt = try XCTUnwrap(persistence.load().clips.first?.createdAt)

        // A schema-8 install may already have the old unversioned backup.
        // Schema 9 must use its own rollback artifact and ignore this one.
        let staleBackup = Data("older-schema-backup".utf8)
        try FileManager.default.createDirectory(
            at: persistence.sharedCoreSchemaBackupURL.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try staleBackup.write(to: persistence.sharedCoreSchemaBackupURL, options: .atomic)

        var store: AppStateStore? = AppStateStore(
            persistence: persistence,
            sharedFeedHost: QueuedCoreFeedHost([]),
            startSubscriptionRefresh: false
        )
        XCTAssertNil(store?.sharedLibraryUnavailableReason)
        XCTAssertEqual(try Data(contentsOf: persistence.sharedCoreSchemaBackupURL), staleBackup)
        XCTAssertEqual(store?.state.clips.map(\.id), [firstID])
        let importedAll = try XCTUnwrap(store?.sharedLibrary).loadClipPages(scope: .all).clips
        XCTAssertEqual(importedAll.prefix(2).map(\.id), [secondID, firstID])
        let imported = try XCTUnwrap(importedAll.first(where: { $0.id == firstID }))
        XCTAssertEqual(imported.revision, 1)
        XCTAssertEqual(imported.episodeID, episodeID)
        XCTAssertEqual(imported.subscriptionID, podcastID)
        XCTAssertEqual(imported.startMs, 12_345)
        XCTAssertEqual(imported.endMs, 15_678)
        XCTAssertEqual(imported.caption, "Legacy exact moment")
        XCTAssertEqual(imported.speakerID, "Speaker One")
        XCTAssertEqual(imported.transcriptText, "The exact frozen transcript words.")
        XCTAssertEqual(imported.source, .touch)
        XCTAssertEqual(
            imported.createdAt.timeIntervalSince1970,
            persistedCreatedAt.timeIntervalSince1970,
            accuracy: 0.001
        )
        XCTAssertTrue(try XCTUnwrap(importedAll.first(where: { $0.id == secondID })).deleted)
        XCTAssertTrue(try persistence.load().clips.isEmpty, "Swift metadata must stop persisting clips")

        let created = try XCTUnwrap(store?.addClip(
            episodeID: episodeID,
            subscriptionID: podcastID,
            startMs: 40_000,
            endMs: 44_000,
            transcriptText: "Captured before refinement.",
            speakerID: speakerID,
            source: .agent,
            caption: "Agent moment"
        ))
        let stale = created
        XCTAssertTrue(store?.updateClipBoundaries(
            id: created.id,
            startMs: 39_500,
            endMs: 44_500,
            transcriptText: "Captured and refined exact words.",
            speakerID: speakerID
        ) == true)
        let refined = try XCTUnwrap(store?.clip(id: created.id))
        XCTAssertEqual(refined.revision, 2)
        XCTAssertEqual(refined.startMs, 39_500)
        XCTAssertEqual(refined.endMs, 44_500)
        XCTAssertEqual(refined.transcriptText, "Captured and refined exact words.")
        XCTAssertEqual(store?.clips(forEpisode: episodeID).map(\.id), [created.id, firstID])
        let sharedLibrary = try XCTUnwrap(store?.sharedLibrary)
        XCTAssertThrowsError(try sharedLibrary.updateClip(stale)) { error in
            XCTAssertEqual(error as? SharedLibraryError, .revisionConflict)
        }
        XCTAssertEqual(store?.clip(id: created.id)?.revision, 2)
        XCTAssertTrue(store?.deleteClip(id: created.id) == true)
        XCTAssertNil(store?.clip(id: created.id))

        store = nil
        store = AppStateStore(
            persistence: persistence,
            sharedFeedHost: QueuedCoreFeedHost([]),
            startSubscriptionRefresh: false
        )
        XCTAssertNil(store?.sharedLibraryUnavailableReason)
        let recoveredAll = try XCTUnwrap(store?.sharedLibrary).loadClipPages(scope: .all).clips
        XCTAssertEqual(Set(recoveredAll.map(\.id)), Set([firstID, secondID, created.id]))
        XCTAssertTrue(try XCTUnwrap(recoveredAll.first(where: { $0.id == created.id })).deleted)
        XCTAssertEqual(store?.allClips().map(\.id), [firstID])

        XCTAssertTrue(store?.clearAllClips() == true)
        XCTAssertTrue(store?.allClips().isEmpty == true)
        store = nil
        let relaunched = AppStateStore(
            persistence: persistence,
            sharedFeedHost: QueuedCoreFeedHost([]),
            startSubscriptionRefresh: false
        )
        XCTAssertNil(relaunched.sharedLibraryUnavailableReason)
        XCTAssertTrue(relaunched.allClips().isEmpty)
        XCTAssertTrue(relaunched.state.clips.isEmpty)
        let tombstones = try XCTUnwrap(relaunched.sharedLibrary).loadClipPages(scope: .all).clips
        XCTAssertEqual(tombstones.count, 3, "Rust clear preserves revisioned tombstones")
        XCTAssertTrue(tombstones.allSatisfy(\.deleted))
    }
}
