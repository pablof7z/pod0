import Foundation
import XCTest
@testable import Podcastr

@MainActor
final class DerivedProjectionConvergenceTests: XCTestCase {
    func testRepeatedReconciliationDoesNotResaveEqualDerivedProjections() throws {
        let made = AppStateTestSupport.makeIsolatedStore()
        defer { AppStateTestSupport.disposeIsolatedStore(at: made.fileURL) }
        let store = made.store
        let database = store.persistence.episodeStore.fileURL
        let jobs = JobStore(fileURL: database)
        let artifacts = ArtifactRepository(fileURL: database)
        try jobs.removeAll()
        let episode = Episode(
            podcastID: UUID(),
            guid: "projection-convergence",
            title: "Projection convergence",
            pubDate: Date(),
            enclosureURL: URL(string: "https://example.com/projection.mp3")!
        )
        store.installEpisodeFixtures([episode], forPodcast: episode.podcastID)
        let chapters = [Episode.Chapter(
            startTime: 0,
            title: "Generated",
            isAIGenerated: true
        )]
        let ads = [Episode.AdSegment(start: 10, end: 20, kind: .midroll)]
        let chapterURL = try adopt(
            chapters, kind: .chapters, episodeID: episode.id, into: artifacts
        )
        let adsURL = try adopt(
            ads, kind: .adSegments, episodeID: episode.id, into: artifacts
        )
        defer {
            try? FileManager.default.removeItem(at: chapterURL)
            try? FileManager.default.removeItem(at: adsURL)
        }
        let reconciler = Reconciler(appStore: store, jobStore: jobs, artifacts: artifacts)
        store.persistence.resetSaveInvocationCount()

        _ = try reconciler.reconcile()
        let firstPassSaves = store.persistence.saveInvocationCount
        XCTAssertEqual(firstPassSaves, 2)
        XCTAssertEqual(store.episode(id: episode.id)?.chapters, chapters)
        XCTAssertEqual(store.episode(id: episode.id)?.adSegments, ads)

        _ = try reconciler.reconcile()
        XCTAssertEqual(store.persistence.saveInvocationCount, firstPassSaves)
        let reloaded = try Persistence(fileURL: made.fileURL).load()
        XCTAssertEqual(reloaded.episodes.first?.chapters, chapters)
        XCTAssertEqual(reloaded.episodes.first?.adSegments, ads)
    }

    func testVerifiedEmptyDerivedArtifactsConvergeWithoutLosingEmptyAdsMeaning() throws {
        let made = AppStateTestSupport.makeIsolatedStore()
        defer { AppStateTestSupport.disposeIsolatedStore(at: made.fileURL) }
        let store = made.store
        let database = store.persistence.episodeStore.fileURL
        let jobs = JobStore(fileURL: database)
        let artifacts = ArtifactRepository(fileURL: database)
        try jobs.removeAll()
        let episode = Episode(
            podcastID: UUID(),
            guid: "empty-projections",
            title: "Empty projections",
            pubDate: Date(),
            enclosureURL: URL(string: "https://example.com/empty.mp3")!
        )
        store.installEpisodeFixtures([episode], forPodcast: episode.podcastID)
        let chapterURL = try adopt(
            [Episode.Chapter](),
            kind: .chapters,
            episodeID: episode.id,
            into: artifacts
        )
        let adsURL = try adopt(
            [Episode.AdSegment](),
            kind: .adSegments,
            episodeID: episode.id,
            into: artifacts
        )
        defer {
            try? FileManager.default.removeItem(at: chapterURL)
            try? FileManager.default.removeItem(at: adsURL)
        }
        let reconciler = Reconciler(appStore: store, jobStore: jobs, artifacts: artifacts)
        store.persistence.resetSaveInvocationCount()

        _ = try reconciler.reconcile()
        let firstPassSaves = store.persistence.saveInvocationCount
        XCTAssertEqual(firstPassSaves, 1)
        XCTAssertNil(store.episode(id: episode.id)?.chapters)
        XCTAssertEqual(store.episode(id: episode.id)?.adSegments, [])

        _ = try reconciler.reconcile()
        XCTAssertEqual(store.persistence.saveInvocationCount, firstPassSaves)
    }

    private func adopt<T: Encodable>(
        _ value: T,
        kind: ArtifactKind,
        episodeID: UUID,
        into repository: ArtifactRepository
    ) throws -> URL {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.sortedKeys]
        let data = try encoder.encode(value)
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("\(UUID().uuidString).json")
        try data.write(to: url, options: .atomic)
        try repository.adopt(ArtifactRecord(
            kind: kind,
            subjectID: episodeID,
            inputVersion: "projection-v1",
            outputVersion: ArtifactRepository.hash(data),
            contentHash: ArtifactRepository.hash(data),
            location: url.path,
            origin: "test",
            schemaVersion: 1,
            integrity: .available,
            verifiedAt: Date()
        ))
        return url
    }
}
