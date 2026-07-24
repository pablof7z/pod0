import Foundation
import XCTest
@testable import Podcastr

@MainActor
final class SharedProjectionPersistenceBoundaryTests: XCTestCase {
    func testProjectionMutationRebuildsReadModelWithoutNativeSave() {
        let made = AppStateTestSupport.makeIsolatedStore()
        defer { AppStateTestSupport.disposeIsolatedStore(at: made.fileURL) }
        let podcast = Podcast(
            feedURL: URL(string: "https://projection.example/feed.xml")!,
            title: "Projection",
            discoveredAt: Date(timeIntervalSince1970: 1_700_000_000)
        )
        let episode = Episode(
            podcastID: podcast.id,
            guid: "projected",
            title: "Projected episode",
            pubDate: Date(timeIntervalSince1970: 1_700_000_100),
            enclosureURL: URL(string: "https://projection.example/episode.mp3")!
        )

        made.store.persistence.resetSaveInvocationCount()
        made.store.mutateProjectionState {
            $0.podcasts = [podcast]
            $0.episodes = [episode]
        }

        XCTAssertEqual(made.store.state.podcasts.map(\.id), [podcast.id])
        XCTAssertEqual(made.store.state.episodes.map(\.id), [episode.id])
        XCTAssertEqual(
            made.store.episodeIndexesByShow[podcast.id],
            [0],
            "Projection-only mutations must still rebuild native lookup caches"
        )
        XCTAssertEqual(made.store.persistence.saveInvocationCount, 0)
    }

    func testEverySharedProjectionAdapterAvoidsNativeSave() {
        let made = AppStateTestSupport.makeIsolatedStore()
        defer { AppStateTestSupport.disposeIsolatedStore(at: made.fileURL) }
        let note = Note(text: "Projected note")
        let memory = AgentMemory(content: "Projected memory")
        let task = AgentScheduledTask(
            id: UUID(),
            label: "Projected task",
            prompt: "Summarize the library",
            intervalSeconds: 3_600,
            createdAt: Date(timeIntervalSince1970: 1_700_000_000),
            lastRunAt: nil,
            nextRunAt: Date(timeIntervalSince1970: 1_700_003_600)
        )

        made.store.persistence.resetSaveInvocationCount()
        made.store.applySharedNotes(SharedNoteSnapshot(
            collectionRevision: .init(value: 2),
            notes: [note],
            operations: []
        ))
        made.store.applySharedMemories(SharedMemorySnapshot(
            collectionRevision: .init(value: 2),
            memories: [memory],
            compiled: nil,
            operations: []
        ))
        made.store.applySharedClips(SharedClipSnapshot(
            collectionRevision: .init(value: 2),
            clips: [],
            operations: []
        ))
        made.store.applySharedScheduledTasks([task])

        XCTAssertEqual(made.store.state.notes.map(\.id), [note.id])
        XCTAssertEqual(made.store.state.agentMemories.map(\.id), [memory.id])
        XCTAssertEqual(made.store.scheduledTasks.map(\.id), [task.id])
        XCTAssertEqual(made.store.persistence.saveInvocationCount, 0)
    }

    func testListeningRowsRemainMigrationInputUntilAuthorityActivates() throws {
        let persistence = Persistence(fileURL: AppStateTestSupport.uniqueTempFileURL())
        defer { persistence.reset() }
        let podcast = Podcast(
            feedURL: URL(string: "https://migration.example/feed.xml")!,
            title: "Migration source",
            discoveredAt: Date(timeIntervalSince1970: 1_700_000_000)
        )
        let episode = Episode(
            podcastID: podcast.id,
            guid: "migration-source",
            title: "Migration source episode",
            pubDate: Date(timeIntervalSince1970: 1_700_000_100),
            enclosureURL: URL(string: "https://migration.example/episode.mp3")!
        )
        var state = AppState()
        state.podcasts = [podcast]
        state.subscriptions = [PodcastSubscription(podcastID: podcast.id)]
        state.episodes = [episode]
        state.lastPlayedEpisodeID = episode.id

        XCTAssertTrue(persistence.write(state, revision: 1))
        let migrationInput = try persistence.load()
        XCTAssertEqual(
            Set(migrationInput.podcasts.map(\.id)),
            Set([podcast.id, Podcast.unknownID])
        )
        XCTAssertEqual(migrationInput.subscriptions.map(\.podcastID), [podcast.id])
        XCTAssertEqual(migrationInput.episodes.map(\.id), [episode.id])
        XCTAssertEqual(migrationInput.lastPlayedEpisodeID, episode.id)

        persistence.activateSharedListeningAuthority()
        XCTAssertTrue(persistence.write(state, revision: 2))
        let nativeAfterCutover = try persistence.load()
        XCTAssertTrue(nativeAfterCutover.subscriptions.isEmpty)
        XCTAssertTrue(nativeAfterCutover.episodes.isEmpty)
        XCTAssertNil(nativeAfterCutover.lastPlayedEpisodeID)
        XCTAssertEqual(
            nativeAfterCutover.podcasts.map(\.id),
            [Podcast.unknownID],
            "Decoder keeps only the synthetic Unknown row, not a durable listening mirror"
        )
    }

    func testDirectMigrationWriteAdvancesTheNextNativeRevision() throws {
        let persistence = Persistence(fileURL: AppStateTestSupport.uniqueTempFileURL())
        defer { persistence.reset() }
        var state = AppState()
        state.settings.hasCompletedOnboarding = true

        XCTAssertTrue(persistence.write(state, revision: 7))
        state.settings.hasCompletedOnboarding = false
        XCTAssertEqual(persistence.save(state), 8)
        XCTAssertFalse(try persistence.load().settings.hasCompletedOnboarding)
    }
}
