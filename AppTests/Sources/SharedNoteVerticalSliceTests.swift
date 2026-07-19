import Foundation
import XCTest
@testable import Podcastr

@MainActor
final class SharedNoteVerticalSliceTests: XCTestCase {
    func testLegacyNotesCutOverLosslesslyAndCommandsSurviveRelaunch() throws {
        let fileURL = AppStateTestSupport.uniqueTempFileURL()
        let persistence = Persistence(fileURL: fileURL)
        defer { persistence.reset() }
        let episodeID = UUID(uuidString: "22222222-2222-2222-2222-222222222222")!
        let podcastID = UUID(uuidString: "11111111-1111-1111-1111-111111111111")!
        let firstID = UUID(uuidString: "33333333-3333-3333-3333-333333333333")!
        let secondID = UUID(uuidString: "44444444-4444-4444-4444-444444444444")!
        let createdAt = Date(timeIntervalSince1970: 1_704_153_600.125)
        var legacy = AppState()
        legacy.podcasts = [Podcast(
            id: podcastID,
            feedURL: URL(string: "https://notes.example/feed.xml")!,
            title: "Note Targets",
            discoveredAt: Date(timeIntervalSince1970: 1_700_000_000)
        )]
        legacy.subscriptions = [PodcastSubscription(podcastID: podcastID)]
        legacy.episodes = [Episode(
            id: episodeID,
            podcastID: podcastID,
            guid: "notes-episode",
            title: "Anchored Notes",
            pubDate: Date(timeIntervalSince1970: 1_700_000_100),
            enclosureURL: URL(string: "https://notes.example/episode.mp3")!
        )]
        legacy.notes = [
            Note(
                id: firstID,
                revision: 1,
                text: "Legacy exact thought",
                kind: .reflection,
                target: .episode(id: episodeID, positionSeconds: 12.345),
                createdAt: createdAt,
                deleted: false,
                author: .user,
                evidence: nil
            ),
            Note(
                id: secondID,
                revision: 1,
                text: "Later deleted thought",
                kind: .free,
                target: nil,
                createdAt: Date(timeIntervalSince1970: 1_704_240_000),
                deleted: true,
                author: .agent,
                evidence: nil
            )
        ]
        XCTAssertTrue(persistence.write(legacy, revision: 7))
        let persistedCreatedAt = try XCTUnwrap(persistence.load().notes.first?.createdAt)

        var store: AppStateStore? = AppStateStore(
            persistence: persistence,
            sharedFeedHost: QueuedCoreFeedHost([]),
            startSubscriptionRefresh: false
        )
        XCTAssertNil(store?.sharedLibraryUnavailableReason)
        let imported = try XCTUnwrap(store?.state.notes.first(where: { $0.id == firstID }))
        XCTAssertEqual(imported.id, firstID)
        XCTAssertEqual(imported.text, "Legacy exact thought")
        XCTAssertEqual(imported.kind, .reflection)
        XCTAssertEqual(imported.author, .user)
        XCTAssertEqual(
            imported.createdAt.timeIntervalSince1970,
            persistedCreatedAt.timeIntervalSince1970,
            accuracy: 0.001
        )
        guard case .episode(let importedEpisodeID, let importedPosition) = imported.target else {
            return XCTFail("Expected preserved episode anchor")
        }
        XCTAssertEqual(importedEpisodeID, episodeID)
        XCTAssertEqual(importedPosition, 12.345, accuracy: 0.000_1)
        XCTAssertEqual(store?.state.notes.prefix(2).map(\.id), [secondID, firstID])
        let importedDeleted = try XCTUnwrap(store?.state.notes.first(where: { $0.id == secondID }))
        XCTAssertTrue(importedDeleted.deleted)
        XCTAssertEqual(importedDeleted.author, .agent)
        XCTAssertTrue(try persistence.load().notes.isEmpty, "Swift metadata must stop persisting notes")

        let later = try XCTUnwrap(store?.addNote(
            text: "Later note",
            target: .episode(id: episodeID, positionSeconds: 30),
            author: .agent
        ))
        let earlier = try XCTUnwrap(store?.addNote(
            text: "Earlier note",
            target: .episode(id: episodeID, positionSeconds: 5),
            author: .user
        ))
        XCTAssertEqual(store?.notes(forEpisode: episodeID).map(\.id), [earlier.id, firstID, later.id])

        var edited = later
        edited.text = "Later note, edited"
        XCTAssertTrue(store?.updateNote(edited) == true)
        XCTAssertEqual(store?.state.notes.first(where: { $0.id == later.id })?.revision, 2)
        XCTAssertFalse(store?.updateNote(later) == true, "A stale projection must not overwrite Rust")
        XCTAssertEqual(
            store?.state.notes.first(where: { $0.id == later.id })?.text,
            "Later note, edited"
        )
        XCTAssertTrue(store?.deleteNote(earlier.id) == true)
        XCTAssertFalse(store?.notes(forEpisode: episodeID).contains(where: { $0.id == earlier.id }) == true)
        XCTAssertTrue(store?.restoreNote(earlier.id) == true)

        store = nil
        store = AppStateStore(
            persistence: persistence,
            sharedFeedHost: QueuedCoreFeedHost([]),
            startSubscriptionRefresh: false
        )
        XCTAssertEqual(
            Set(store?.state.notes.map(\.id) ?? []),
            Set([firstID, secondID, later.id, earlier.id])
        )
        XCTAssertEqual(
            store?.state.notes.first(where: { $0.id == later.id })?.text,
            "Later note, edited"
        )

        XCTAssertTrue(store?.clearAllNotes() == true)
        XCTAssertTrue(store?.activeNotes.isEmpty == true)
        store = nil
        let relaunched = AppStateStore(
            persistence: persistence,
            sharedFeedHost: QueuedCoreFeedHost([]),
            startSubscriptionRefresh: false
        )
        XCTAssertTrue(relaunched.activeNotes.isEmpty)
        XCTAssertEqual(relaunched.state.notes.count, 4, "Clear preserves revisioned tombstones")
    }
}
