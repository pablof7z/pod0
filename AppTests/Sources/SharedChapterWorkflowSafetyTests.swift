import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class SharedChapterWorkflowSafetyTests: XCTestCase {
    private var cleanupURLs: [URL] = []

    override func tearDown() async throws {
        cleanupURLs.forEach(AppStateTestSupport.disposeIsolatedStore(at:))
        cleanupURLs.removeAll()
        try await super.tearDown()
    }

    func testDelayedWorkflowCommitCannotOverwriteNewerChapterSelection() throws {
        let fixture = makeStore()
        defer { dispose(fixture) }
        let client = try XCTUnwrap(fixture.store.sharedLibrary)
        let delayed = try publisherQualification(
            episode: fixture.episode,
            title: "Delayed publisher response"
        )
        let current = try client.submitChapterObservation(
            try publisherQualification(
                episode: fixture.episode,
                title: "Newer selection"
            ),
            expectedSelectionRevision: StateRevision(value: 0)
        )

        XCTAssertThrowsError(try client.submitChapterObservation(
            delayed,
            expectedSelectionRevision: StateRevision(value: 0)
        )) { error in
            XCTAssertEqual(error as? SharedLibraryError, .revisionConflict)
        }
        XCTAssertEqual(
            try client.authoritativeChapterReader.summary(
                episodeID: fixture.episode.id
            )?.artifactId,
            current.receipt.artifactId
        )
    }

    func testInjectedStoreDoesNotPublishSettingsToProcessWideICloudChannel() {
        let key = "sync.settings.chapterCompilationModel"
        let remoteBefore = NSUbiquitousKeyValueStore.default.object(forKey: key) as? String
        let fixture = makeStore()
        defer { dispose(fixture) }

        XCTAssertFalse(fixture.store.syncSettingsWithICloud)
        var changed = fixture.store.state.settings
        changed.chapterCompilationModel = "test/local-only-model"
        fixture.store.updateSettings(changed)

        XCTAssertEqual(
            NSUbiquitousKeyValueStore.default.object(forKey: key) as? String,
            remoteBefore
        )
    }

    func testInitialPublisherOpportunitySkipsTenThousandSourceFreeEpisodes() {
        let episodes = (0..<10_000).map { projectedEpisode(index: $0, chaptersURL: nil) }
        let snapshot = librarySnapshot(episodes: episodes)
        let requested = episodes.compactMap { $0.episodeId.uuid }

        XCTAssertTrue(PublisherChapterOpportunityPlanner.changedEpisodeIDs(
            previous: nil,
            current: snapshot
        ).isEmpty)
        XCTAssertTrue(PublisherChapterOpportunityPlanner.requestedEpisodeIDs(
            requested: requested,
            current: snapshot,
            excluding: []
        ).isEmpty)
    }

    func testInitialPublisherOpportunityIncludesOnlyNonEmptySourcesOnce() throws {
        let missing = projectedEpisode(index: 1, chaptersURL: nil)
        let blank = projectedEpisode(index: 2, chaptersURL: " \n ")
        let sourced = projectedEpisode(index: 3, chaptersURL: "https://example.com/chapters.json")
        let snapshot = librarySnapshot(episodes: [missing, blank, sourced])
        let sourcedID = try XCTUnwrap(sourced.episodeId.uuid)
        let requested = [missing, blank, sourced].compactMap { $0.episodeId.uuid }

        XCTAssertEqual(PublisherChapterOpportunityPlanner.changedEpisodeIDs(
            previous: nil,
            current: snapshot
        ), [sourcedID])
        let first = PublisherChapterOpportunityPlanner.requestedEpisodeIDs(
            requested: requested,
            current: snapshot,
            excluding: []
        )
        XCTAssertEqual(first, [sourcedID])
        XCTAssertTrue(PublisherChapterOpportunityPlanner.requestedEpisodeIDs(
            requested: requested,
            current: snapshot,
            excluding: Set(first)
        ).isEmpty)
    }

    func testPublisherSourceAddReplaceAndRemovalAreAnnounced() {
        let previous = librarySnapshot(episodes: [
            projectedEpisode(index: 1, chaptersURL: nil),
            projectedEpisode(index: 2, chaptersURL: "https://example.com/old.json"),
            projectedEpisode(index: 3, chaptersURL: "https://example.com/remove.json"),
            projectedEpisode(index: 4, chaptersURL: nil),
            projectedEpisode(index: 5, chaptersURL: "https://example.com/same.json"),
        ])
        let current = librarySnapshot(episodes: [
            projectedEpisode(index: 1, chaptersURL: "https://example.com/added.json"),
            projectedEpisode(index: 2, chaptersURL: "https://example.com/new.json"),
            projectedEpisode(index: 3, chaptersURL: nil),
            projectedEpisode(index: 4, chaptersURL: nil),
            projectedEpisode(index: 5, chaptersURL: "https://example.com/same.json"),
            projectedEpisode(index: 6, chaptersURL: nil),
        ])

        XCTAssertEqual(
            Set(PublisherChapterOpportunityPlanner.changedEpisodeIDs(
                previous: previous,
                current: current
            )),
            Set(current.episodes.prefix(3).compactMap { $0.episodeId.uuid })
        )
    }

    private func publisherQualification(
        episode: Episode,
        title: String
    ) throws -> ChapterObservationProjection {
        let payload = Data(
            #"{"version":"1.2.0","chapters":[{"startTime":0,"title":"\#(title)"}]}"#.utf8
        )
        let digest = try XCTUnwrap(ContentDigest(
            hexadecimal: ArtifactRepository.hash(payload)
        ))
        return qualifyPublisherChapterObservation(observation: PublisherChapterObservation(
            episodeId: EpisodeId(uuid: episode.id),
            podcastId: PodcastId(uuid: episode.podcastID),
            resolvedSourceUrl: try XCTUnwrap(episode.chaptersURL).absoluteString,
            contentType: "application/json",
            payloadDigest: digest,
            payload: payload,
            generatedAt: UnixTimestampMilliseconds(value: 1_700_000_000_000),
            durationMilliseconds: 60_000
        ))
    }

    private func librarySnapshot(episodes: [EpisodeRecord]) -> SharedLibrarySnapshot {
        SharedLibrarySnapshot(
            podcasts: [],
            subscriptions: [],
            episodes: episodes,
            chaptersByEpisodeID: [:],
            operations: []
        )
    }

    private func projectedEpisode(index: Int, chaptersURL: String?) -> EpisodeRecord {
        EpisodeRecord(
            episodeId: EpisodeId(high: 0xA11CE, low: UInt64(index + 1)),
            podcastId: PodcastId(high: 0xB0D0, low: 1),
            publisherGuid: "episode-\(index)",
            title: "Episode \(index)",
            description: "",
            publishedAt: UnixTimestampMilliseconds(value: Int64(index)),
            durationMilliseconds: nil,
            enclosureUrl: "https://example.com/\(index).mp3",
            enclosureMimeType: "audio/mpeg",
            imageUrl: nil,
            feedMetadata: EpisodeFeedMetadata(
                publisherTranscript: nil,
                chaptersUrl: chaptersURL,
                persons: [],
                soundBites: []
            ),
            listening: EpisodeListeningState(
                resumePositionMilliseconds: 0,
                completion: .inProgress
            ),
            isStarred: false,
            download: .unavailable,
            transcript: .unavailable
        )
    }

    private func makeStore() -> Fixture {
        let fileURL = AppStateTestSupport.uniqueTempFileURL()
        cleanupURLs.append(fileURL)
        let persistence = Persistence(fileURL: fileURL)
        let podcast = Podcast(
            id: UUID(),
            feedURL: URL(string: "https://workflow.example/feed.xml")!,
            title: "Workflow Safety"
        )
        var episode = Episode(
            podcastID: podcast.id,
            guid: "workflow-safety",
            title: "Workflow Safety",
            pubDate: Date(timeIntervalSince1970: 1_700_000_000),
            duration: 60,
            enclosureURL: URL(string: "https://workflow.example/audio.mp3")!
        )
        episode.chaptersURL = URL(string: "https://workflow.example/chapters.json")!
        var state = AppState()
        state.podcasts = [podcast]
        state.subscriptions = [PodcastSubscription(podcastID: podcast.id)]
        state.episodes = [episode]
        XCTAssertTrue(persistence.write(state, revision: 1))
        return Fixture(
            store: AppStateStore(
                persistence: persistence,
                sharedFeedHost: QueuedCoreFeedHost([]),
                startSubscriptionRefresh: false
            ),
            fileURL: fileURL,
            episode: episode
        )
    }

    private func dispose(_ fixture: Fixture) {
        fixture.store.sharedLibrary?.shutdown()
    }

    private struct Fixture {
        let store: AppStateStore
        let fileURL: URL
        let episode: Episode
    }
}
