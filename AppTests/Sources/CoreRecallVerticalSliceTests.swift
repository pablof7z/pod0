import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class CoreRecallVerticalSliceTests: XCTestCase {
    func testPreparedTranscriptRebuildSurvivesRestartWithoutChangingSelectedFile() async throws {
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
        let index = try VectorIndex(embedder: embedder, inMemory: true, dimensions: 3)

        guard case .ready(let first) = SharedLibraryBootstrap.run(
            persistence: persistence,
            feedHost: VerticalSliceFeedHost()
        ) else { return XCTFail("Expected an authoritative shared store") }
        await attachRecall(to: first, index: index, embedder: embedder)
        let firstResult: OperationResult?
        do {
            firstResult = try await first.rebuildTranscriptEvidence(
                transcript: transcript,
                podcastID: podcastID,
                selectedData: selectedData
            )
        } catch {
            let operation = libraryOperations(first.facade).last
            XCTFail("Evidence rebuild failed: \(String(describing: operation?.failure?.code))")
            return
        }
        guard let firstResult,
              case .evidenceRebuilt(let rebuiltEpisode, let generation, let spanCount) = firstResult else {
            return XCTFail("Expected a completed Rust evidence rebuild")
        }
        XCTAssertEqual(rebuiltEpisode, EpisodeId(uuid: episodeID))
        XCTAssertGreaterThan(spanCount, 0)
        XCTAssertEqual(try Data(contentsOf: selectedURL), selectedData)
        first.shutdown()

        let reopenedFacade = try Pod0Facade.open(storePath: persistence.sharedCoreStoreURL.path)
        guard case .evidenceIndex(let restored) = reopenedFacade.snapshot(request: ProjectionRequest(
            scope: .evidenceIndex(episodeId: EpisodeId(uuid: episodeID)),
            offset: 0,
            maxItems: 16
        )).projection else { return XCTFail("Expected restored evidence projection") }
        XCTAssertEqual(restored.stage, .ready)
        XCTAssertEqual(restored.generationId, generation)
        XCTAssertEqual(restored.totalSpans, spanCount)

        let reopened = await makeClient(
            facade: reopenedFacade,
            index: index,
            embedder: embedder
        )
        _ = try await reopened.rebuildTranscriptEvidence(
            transcript: transcript,
            podcastID: podcastID,
            selectedData: selectedData
        )
        let embeddingCalls = await embedder.callCount
        XCTAssertEqual(embeddingCalls, 1)
        XCTAssertEqual(try Data(contentsOf: selectedURL), selectedData)
        reopened.shutdown()
    }

    private func makeClient(
        facade: Pod0Facade,
        index: VectorIndex,
        embedder: RestartCountingEmbedder
    ) async -> SharedLibraryClient {
        let client = SharedLibraryClient(facade: facade, feedHost: VerticalSliceFeedHost())
        await attachRecall(to: client, index: index, embedder: embedder)
        client.start()
        return client
    }

    private func attachRecall(
        to client: SharedLibraryClient,
        index: VectorIndex,
        embedder: RestartCountingEmbedder
    ) async {
        await client.deferredRecallHost.attach(CoreRecallHost(
            projections: client.facade,
            index: index,
            embedder: embedder,
            reranker: VerticalSliceReranker(),
            isRerankingEnabled: { false }
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
        return texts.map { _ in [1, 0, 0] }
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
