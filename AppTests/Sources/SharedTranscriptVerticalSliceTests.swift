import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class SharedTranscriptVerticalSliceTests: XCTestCase {
    func testTypedSubmissionPagesAndRelaunchProjection() throws {
        let made = makeStore()
        defer { dispose(made) }
        let client = try XCTUnwrap(made.store.sharedLibrary)
        let transcript = makeTranscript(episodeID: made.episodeID)
        let context = makeContext(podcastID: made.podcastID)

        let result = try client.submitTranscriptObservation(transcript, context: context)
        XCTAssertTrue(result.mismatches.isEmpty)
        XCTAssertEqual(result.receipt.selectionRevision.value, 1)

        let reader = SharedTranscriptReader(facade: client.facade)
        let firstPage = try reader.segmentsPage(
            episodeID: transcript.episodeID,
            offset: 0,
            maxItems: 1
        )
        XCTAssertEqual(firstPage.items.count, 1)
        XCTAssertTrue(firstPage.hasMore)
        let exact = try XCTUnwrap(reader.exactSegment(
            episodeID: transcript.episodeID,
            segmentID: firstPage.items[0].coreID
        ))
        XCTAssertEqual(exact.value.text, transcript.segments[0].text)
        let words = try reader.wordsPage(
            episodeID: transcript.episodeID,
            segmentID: exact.coreID,
            offset: 0,
            maxItems: 1
        )
        XCTAssertEqual(words.items.first?.text, "Hello")
        XCTAssertTrue(words.hasMore)

        let reopened = try Pod0Facade.open(
            storePath: made.store.persistence.sharedCoreStoreURL.path
        )
        let relaunched = SharedTranscriptReader(facade: reopened)
        let restored = try XCTUnwrap(relaunched.loadThrowing(episodeID: transcript.episodeID))
        XCTAssertEqual(restored.episodeID, transcript.episodeID)
        XCTAssertEqual(restored.segments.map(\.text), transcript.segments.map(\.text))
        XCTAssertEqual(
            try relaunched.summary(episodeID: transcript.episodeID)?.selectionRevision,
            result.receipt.selectionRevision
        )
    }

    func testLargeTranscriptIsReadThroughBoundedPages() throws {
        let made = makeStore()
        defer { dispose(made) }
        let client = try XCTUnwrap(made.store.sharedLibrary)
        let transcript = Transcript(
            episodeID: made.episodeID,
            language: "en",
            source: .publisher,
            segments: (0..<205).map { index in
                Segment(
                    start: Double(index),
                    end: Double(index + 1),
                    text: "Segment \(index)"
                )
            }
        )
        _ = try client.submitTranscriptObservation(
            transcript,
            context: makeContext(podcastID: made.podcastID)
        )
        let reader = SharedTranscriptReader(facade: client.facade)
        let first = try reader.segmentsPage(
            episodeID: transcript.episodeID,
            offset: 0,
            maxItems: 500
        )
        let second = try reader.segmentsPage(
            episodeID: transcript.episodeID,
            offset: 200,
            maxItems: 500
        )
        XCTAssertEqual(first.items.count, 200)
        XCTAssertTrue(first.hasMore)
        XCTAssertEqual(second.items.count, 5)
        XCTAssertFalse(second.hasMore)
    }

    func testStaleRevisionIsTypedAndNeverBlindlyRetried() throws {
        let made = makeStore()
        defer { dispose(made) }
        let client = try XCTUnwrap(made.store.sharedLibrary)
        let transcript = makeTranscript(episodeID: made.episodeID)
        let context = makeContext(podcastID: made.podcastID)
        let first = try client.submitTranscriptObservation(transcript, context: context)

        XCTAssertThrowsError(try client.submitTranscriptObservation(
            transcript,
            context: context,
            expectedSelectionRevision: StateRevision(value: 0)
        )) { error in
            XCTAssertEqual(error as? SharedLibraryError, .revisionConflict)
        }
        let summary = try SharedTranscriptReader(facade: client.facade)
            .summary(episodeID: transcript.episodeID)
        XCTAssertEqual(summary?.selectionRevision, first.receipt.selectionRevision)
    }

    func testCancellationBeforeCommitLeavesNoSelection() async throws {
        let made = makeStore()
        defer { dispose(made) }
        let client = try XCTUnwrap(made.store.sharedLibrary)
        let transcript = makeTranscript(episodeID: made.episodeID)
        let context = makeContext(podcastID: made.podcastID)

        let cancelled = await Task { @MainActor in
            withUnsafeCurrentTask { $0?.cancel() }
            do {
                _ = try client.submitTranscriptObservation(transcript, context: context)
                return false
            } catch is CancellationError {
                return true
            } catch {
                return false
            }
        }.value
        XCTAssertTrue(cancelled)
        XCTAssertNil(try SharedTranscriptReader(facade: client.facade)
            .summary(episodeID: transcript.episodeID))
    }

    func testUnavailableCoreLeavesSwiftAuthorityReadable() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "transcript-fallback-\(UUID().uuidString)", isDirectory: true
        )
        defer { try? FileManager.default.removeItem(at: root) }
        let legacy = try TranscriptStore(rootDirectory: root)
        let transcript = makeTranscript(episodeID: UUID())
        try legacy.save(transcript)

        await SharedTranscriptShadowObserver.observe(
            transcript: transcript,
            podcastID: UUID(),
            sourceRevision: "audio-v1",
            sourcePayloadDigest: makeContext(podcastID: UUID()).sourcePayloadDigest,
            provider: nil,
            client: nil
        )

        let restored = try XCTUnwrap(legacy.load(episodeID: transcript.episodeID))
        XCTAssertEqual(restored.id, transcript.id)
        XCTAssertEqual(restored.source, transcript.source)
        XCTAssertEqual(restored.segments, transcript.segments)
    }

    func testShadowComparatorClassifiesMismatchWithoutPayloadValues() throws {
        let made = makeStore()
        defer { dispose(made) }
        let client = try XCTUnwrap(made.store.sharedLibrary)
        let authoritative = makeTranscript(episodeID: made.episodeID)
        let context = makeContext(podcastID: made.podcastID)
        _ = try client.submitTranscriptObservation(authoritative, context: context)
        let reader = SharedTranscriptReader(facade: client.facade)
        let summary = try XCTUnwrap(reader.summary(episodeID: authoritative.episodeID))
        let changed = Transcript(
            episodeID: UUID(),
            language: "fr",
            source: .whisper,
            segments: [Segment(
                start: 9,
                end: 10,
                speakerID: UUID(),
                text: "Different",
                words: [Word(start: 9, end: 10, text: "Changed")]
            )],
            speakers: [Speaker(label: "changed")],
            generatedAt: authoritative.generatedAt.addingTimeInterval(1)
        )
        let mismatches = SharedTranscriptShadowComparator.compare(
            authoritative: authoritative,
            podcastID: made.podcastID,
            context: context,
            summary: summary,
            candidate: changed
        )
        XCTAssertEqual(mismatches, Set(TranscriptShadowMismatch.allCases))
    }

    private func makeTranscript(episodeID: UUID) -> Transcript {
        let speaker = Speaker(label: "host", displayName: "Host")
        return Transcript(
            episodeID: episodeID,
            language: "en-US",
            source: .scribeV1,
            segments: [
                Segment(
                    start: 0, end: 2, speakerID: speaker.id, text: "Hello world",
                    words: [
                        Word(start: 0, end: 0.5, text: "Hello"),
                        Word(start: 0.5, end: 1, text: "world"),
                    ]
                ),
                Segment(start: 1.5, end: 3, speakerID: speaker.id, text: "Overlapping cue"),
            ],
            speakers: [speaker],
            generatedAt: Date(timeIntervalSince1970: 1_700_000_000.125)
        )
    }

    private func makeContext(podcastID: UUID) -> TranscriptObservationContext {
        TranscriptObservationContext(
            podcastID: podcastID,
            sourceRevision: "audio-v1",
            sourcePayloadDigest: ArtifactRepository.hash(Data("selected-json".utf8)),
            provider: nil
        )
    }

    private struct StoreFixture {
        let store: AppStateStore
        let fileURL: URL
        let podcastID: UUID
        let episodeID: UUID
    }

    private func makeStore() -> StoreFixture {
        let fileURL = AppStateTestSupport.uniqueTempFileURL()
        let persistence = Persistence(fileURL: fileURL)
        let podcast = Podcast(
            id: UUID(),
            feedURL: URL(string: "https://transcript.example/feed.xml")!,
            title: "Transcript Show",
            discoveredAt: Date(timeIntervalSince1970: 1_700_000_000)
        )
        let episode = Episode(
            id: UUID(),
            podcastID: podcast.id,
            guid: "typed-transcript",
            title: "Typed Transcript",
            pubDate: Date(timeIntervalSince1970: 1_700_000_100),
            enclosureURL: URL(string: "https://transcript.example/episode.mp3")!
        )
        var state = AppState()
        state.podcasts = [podcast]
        state.subscriptions = [PodcastSubscription(podcastID: podcast.id)]
        state.episodes = [episode]
        XCTAssertTrue(persistence.write(state, revision: 1))
        return StoreFixture(
            store: AppStateStore(
                persistence: persistence,
                sharedFeedHost: QueuedCoreFeedHost([]),
                startSubscriptionRefresh: false
            ),
            fileURL: fileURL,
            podcastID: podcast.id,
            episodeID: episode.id
        )
    }

    private func dispose(_ fixture: StoreFixture) {
        fixture.store.sharedLibrary?.shutdown()
        AppStateTestSupport.disposeIsolatedStore(at: fixture.fileURL)
    }
}
