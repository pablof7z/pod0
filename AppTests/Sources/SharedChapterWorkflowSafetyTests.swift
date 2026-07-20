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

    func testVerifierKeepsCurrentChapterJobAfterRustSelectionCommit() throws {
        let fixture = makeStore()
        defer { dispose(fixture) }
        let client = try XCTUnwrap(fixture.store.sharedLibrary)
        let transcript = Transcript(
            episodeID: fixture.episode.id,
            language: "en",
            source: .publisher,
            segments: [Segment(start: 0, end: 5, text: "Current transcript")]
        )
        _ = try client.submitTranscriptObservation(
            transcript,
            context: TranscriptObservationContext(
                podcastID: fixture.episode.podcastID,
                sourceRevision: DesiredStatePlanner.audioVersion(fixture.episode),
                sourcePayloadDigest: ArtifactRepository.hash(Data("transcript".utf8)),
                provider: nil
            )
        )
        _ = try client.submitChapterObservation(
            try publisherQualification(
                episode: fixture.episode,
                title: "Selected publisher chapter"
            ),
            expectedSelectionRevision: StateRevision(value: 0)
        )
        let snapshot = try XCTUnwrap(
            client.transcriptWorkflowSnapshots(episodeIDs: [fixture.episode.id]).first
        )
        let inputVersion = DesiredStatePlanner.chapterCompilerInputVersion(
            snapshot,
            settings: fixture.store.state.settings
        )
        let jobs = JobStore(fileURL: fixture.store.persistence.episodeStore.fileURL)
        let key = "compile:\(fixture.episode.id):\(inputVersion)"
        _ = try jobs.ensureJob(DesiredJob(
            idempotencyKey: key,
            kind: .chapterArtifacts,
            subjectID: fixture.episode.id,
            inputVersion: inputVersion,
            resourceClass: .utilityLLM
        ))
        let job = try XCTUnwrap(jobs.job(idempotencyKey: key))
        let verifier = WorkflowArtifactVerifier(
            appStore: fixture.store,
            artifacts: ArtifactRepository(fileURL: jobs.fileURL)
        )

        XCTAssertTrue(verifier.isStillCurrent(job))
        var changed = fixture.store.state.settings
        changed.chapterCompilationModel += "-changed"
        fixture.store.updateSettings(changed)
        XCTAssertFalse(verifier.isStillCurrent(job))
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
