import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class CoreRecallVerticalSliceTests: XCTestCase {
    func testPreparedTranscriptRebuildSurvivesRestartWithoutConsultingLegacyFile() async throws {
        let root = URL(fileURLWithPath: NSTemporaryDirectory(), isDirectory: true)
            .appendingPathComponent("pod0-recall-slice-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let stateURL = root.appendingPathComponent("state.json")
        let persistence = Persistence(fileURL: stateURL)
        defer { try? FileManager.default.removeItem(at: root) }
        let episodeID = UUID(uuidString: "51000000-0000-0000-0000-000000000001")!
        let podcastID = UUID(uuidString: "52000000-0000-0000-0000-000000000001")!
        var state = AppState()
        state.podcasts = [Podcast(
            id: podcastID,
            feedURL: URL(string: "https://example.test/evidence.xml")!,
            title: "Evidence Show",
            author: "Pod0"
        )]
        state.episodes = [Episode(
            id: episodeID,
            podcastID: podcastID,
            guid: "evidence-episode",
            title: "Evidence Episode",
            pubDate: Date(timeIntervalSince1970: 1_800_000_000),
            duration: 300,
            enclosureURL: URL(string: "https://example.test/evidence.mp3")!,
            transcriptState: .ready(source: .publisher)
        )]
        XCTAssertEqual(persistence.save(state), 1)
        let selectedURL = root.appendingPathComponent("selected.json")
        let selectedData = Data(#"{"source":"publisher","segments":2}"#.utf8)
        try selectedData.write(to: selectedURL, options: .atomic)

        let transcript = Transcript(
            episodeID: episodeID,
            language: "en-US",
            source: .publisher,
            segments: [
                Segment(start: 12, end: 18, text: "Exact durable evidence begins here."),
                Segment(start: 18, end: 24, text: "The selected transcript remains unchanged."),
            ]
        )
        let embedder = RestartCountingEmbedder()

        guard case .ready(let first) = SharedLibraryBootstrap.run(
            persistence: persistence,
            legacyState: state,
            feedHost: VerticalSliceFeedHost()
        ) else { return XCTFail("Expected an authoritative shared store") }
        await attachRecall(to: first, embedder: embedder)
        let committed = try first.submitTranscriptObservation(
            transcript,
            context: TranscriptObservationContext(
                podcastID: podcastID,
                sourceRevision: "audio-v1",
                sourcePayloadDigest: ArtifactRepository.hash(selectedData),
                provider: "publisher-feed"
            )
        )
        let firstResult: SharedEvidenceReceipt
        do {
            firstResult = try await first.rebuildTranscriptEvidence(
                transcript: transcript,
                summary: committed.summary,
                inputVersion: "audio-v1"
            )
        } catch {
            let operation = libraryOperations(first.facade).last
            XCTFail("Evidence rebuild failed: \(String(describing: operation?.failure?.code))")
            return
        }
        XCTAssertEqual(firstResult.episodeID, episodeID)
        XCTAssertGreaterThan(firstResult.spanCount, 0)
        XCTAssertEqual(try Data(contentsOf: selectedURL), selectedData)
        let firstRecall = await first.recall(
            query: "durable evidence",
            scope: .episode(episodeId: EpisodeId(uuid: episodeID)),
            limit: 3
        )
        XCTAssertEqual(firstRecall.stage, .ready)
        XCTAssertEqual(firstRecall.evidence.first?.generationId.stableString, firstResult.generationID)
        XCTAssertEqual(firstRecall.evidence.first?.startMilliseconds, 12_000)

        let recallStarted = expectation(description: "Recall capability started")
        await first.deferredRecallHost.attach(CancellationRecallHost(started: recallStarted))
        let cancelledTask = Task {
            await first.recall(
                query: "cancel this recall",
                scope: .episode(episodeId: EpisodeId(uuid: episodeID)),
                limit: 3
            )
        }
        await fulfillment(of: [recallStarted], timeout: 1)
        cancelledTask.cancel()
        let cancelled = await cancelledTask.value
        XCTAssertEqual(cancelled.stage, .cancelled)
        XCTAssertTrue(cancelled.evidence.isEmpty)
        first.shutdown()

        let reopenedFacade = try Pod0Facade.open(storePath: persistence.sharedCoreStoreURL.path)
        guard case .evidenceIndex(let restored) = reopenedFacade.snapshot(request: ProjectionRequest(
            scope: .evidenceIndex(episodeId: EpisodeId(uuid: episodeID)),
            offset: 0,
            maxItems: 16
        )).projection else { return XCTFail("Expected restored evidence projection") }
        XCTAssertEqual(restored.stage, .ready)
        XCTAssertEqual(restored.generationId?.stableString, firstResult.generationID)
        XCTAssertEqual(restored.totalSpans, firstResult.spanCount)

        let reopened = await makeClient(
            facade: reopenedFacade,
            coreStoreURL: persistence.sharedCoreStoreURL,
            embedder: embedder
        )
        let reopenedSummary = try XCTUnwrap(
            try SharedTranscriptReader(facade: reopenedFacade).summary(episodeID: episodeID)
        )
        _ = try await reopened.rebuildTranscriptEvidence(
            transcript: transcript,
            summary: reopenedSummary,
            inputVersion: "audio-v1"
        )
        let reopenedRecall = await reopened.recall(
            query: "selected transcript",
            scope: .episode(episodeId: EpisodeId(uuid: episodeID)),
            limit: 3
        )
        XCTAssertEqual(reopenedRecall.stage, .ready)
        XCTAssertEqual(
            reopenedRecall.evidence.first?.generationId.stableString,
            firstResult.generationID
        )
        let embeddingCalls = await embedder.callCount
        XCTAssertEqual(embeddingCalls, 3)
        XCTAssertEqual(try Data(contentsOf: selectedURL), selectedData)
        reopened.shutdown()
    }

    private func makeClient(
        facade: Pod0Facade,
        coreStoreURL: URL,
        embedder: RestartCountingEmbedder
    ) async -> SharedLibraryClient {
        let client = SharedLibraryClient(
            facade: facade,
            coreStoreURL: coreStoreURL,
            feedHost: VerticalSliceFeedHost()
        )
        await attachRecall(to: client, embedder: embedder)
        client.start()
        return client
    }

    private func attachRecall(
        to client: SharedLibraryClient,
        embedder: RestartCountingEmbedder
    ) async {
        await client.deferredRecallHost.attach(CoreRecallHost(
            providers: TestRecallProviderExecutor(
                embedder: embedder,
                reranker: VerticalSliceReranker()
            ),
            legacyIndexURL: FileManager.default.temporaryDirectory
                .appendingPathComponent("pod0-recall-vertical-\(UUID().uuidString)")
                .appendingPathComponent("vectors.sqlite")
        ))
    }

    private func libraryOperations(_ facade: Pod0Facade) -> [OperationProjection] {
        guard case .library(let library) = facade.snapshot(request: ProjectionRequest(
            scope: .library,
            offset: 0,
            maxItems: 20
        )).projection else { return [] }
        return library.operations
    }
}

private actor RestartCountingEmbedder: EmbeddingsClient {
    private(set) var callCount = 0

    func embed(_ texts: [String]) async throws -> [[Float]] {
        callCount += 1
        return texts.map { _ in
            var values = [Float](repeating: 0, count: 1_024)
            values[0] = 1
            return values
        }
    }
}

private struct VerticalSliceReranker: RerankerClient {
    func rerank(query: String, documents: [String], topN: Int?) async throws -> [Int] {
        Array(documents.indices)
    }
}

private struct VerticalSliceFeedHost: CoreFeedHosting {
    func fetch(
        feedURL: String,
        entityTag: String?,
        lastModified: String?,
        maximumResponseBytes: UInt64,
        deadline: Date?
    ) async -> HostObservation {
        .failed(code: .platformFailure, safeDetail: nil)
    }
}

private struct CancellationRecallHost: CoreRecallHosting, @unchecked Sendable {
    let started: XCTestExpectation

    func execute(_ request: HostRequest) async -> HostObservation {
        guard case .embedRecallQuery = request else {
            return .failed(code: .invalidResponse, safeDetail: nil)
        }
        started.fulfill()
        do {
            try await Task.sleep(for: .seconds(30))
            return .failed(code: .timedOut, safeDetail: nil)
        } catch {
            return .cancelled
        }
    }
}
