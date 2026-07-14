import XCTest
@testable import Podcastr

@MainActor
final class EpisodeMetadataIndexerTests: XCTestCase {

    private var fileURL: URL!
    private var store: AppStateStore!

    override func setUp() async throws {
        try await super.setUp()
        let made = AppStateTestSupport.makeIsolatedStore()
        store = made.store
        fileURL = made.fileURL
    }

    override func tearDown() async throws {
        if let fileURL { AppStateTestSupport.disposeIsolatedStore(at: fileURL) }
        store = nil
        fileURL = nil
        try await super.tearDown()
    }

    // MARK: - Backfill

    func testBackfillIndexesEveryUnflaggedEpisodeAndFlipsTheFlag() async {
        let podcastID = UUID()
        let e1 = makeEpisode(podcastID: podcastID, title: "First", description: "<p>About widgets</p>")
        let e2 = makeEpisode(podcastID: podcastID, title: "Second", description: "About gizmos")
        store.upsertEpisodes([e1, e2], forPodcast: podcastID)

        let fake = FakeVectorStore()
        let indexer = EpisodeMetadataIndexer(store: fake)
        await indexer.runBackfill(appStore: store)

        let upserts = await fake.upsertCalls
        XCTAssertEqual(upserts.count, 1, "single batch expected")
        XCTAssertEqual(upserts.first?.count, 2)
        for ep in [e1, e2] {
            XCTAssertTrue(store.episode(id: ep.id)?.metadataIndexed ?? false,
                          "metadataIndexed should flip for \(ep.title)")
        }
    }

    func testBackfillSkipsEpisodesAlreadyFlaggedIndexed() async {
        let podcastID = UUID()
        let stale = makeEpisode(podcastID: podcastID, title: "Old", description: "x",
                                metadataIndexed: true)
        let fresh = makeEpisode(podcastID: podcastID, title: "New", description: "y")
        store.upsertEpisodes([stale, fresh], forPodcast: podcastID)

        let fake = FakeVectorStore()
        let indexer = EpisodeMetadataIndexer(store: fake)
        await indexer.runBackfill(appStore: store)

        let upserts = await fake.upsertCalls
        XCTAssertEqual(upserts.flatMap { $0 }.count, 1, "only the unflagged episode is upserted")
        XCTAssertEqual(upserts.flatMap { $0 }.first?.episodeID, fresh.id)
    }

    func testBackfillSkipsEpisodesWithNoTitleOrDescription() async {
        let podcastID = UUID()
        let empty = makeEpisode(podcastID: podcastID, title: "", description: "   \n  ")
        store.upsertEpisodes([empty], forPodcast: podcastID)

        let fake = FakeVectorStore()
        let indexer = EpisodeMetadataIndexer(store: fake)
        await indexer.runBackfill(appStore: store)

        let upserts = await fake.upsertCalls
        XCTAssertTrue(upserts.flatMap { $0 }.isEmpty)
        XCTAssertFalse(store.episode(id: empty.id)?.metadataIndexed ?? true,
                       "empty episodes are not flagged so a future content refresh can index them")
    }

    func testBackfillLeavesFlagUnchangedWhenUpsertThrows() async {
        let podcastID = UUID()
        let ep = makeEpisode(podcastID: podcastID, title: "Title", description: "Body")
        store.upsertEpisodes([ep], forPodcast: podcastID)

        let fake = FakeVectorStore()
        await fake.setShouldFail(true)
        let indexer = EpisodeMetadataIndexer(store: fake)
        await indexer.runBackfill(appStore: store)

        XCTAssertFalse(store.episode(id: ep.id)?.metadataIndexed ?? true,
                       "embed failure must not falsely flag the episode as indexed")
    }

    func testIndexedChunkContainsTitleAndStrippedDescription() async {
        let podcastID = UUID()
        let ep = makeEpisode(
            podcastID: podcastID,
            title: "Episode Title",
            description: "<p>Hello <b>world</b>.</p>"
        )
        store.upsertEpisodes([ep], forPodcast: podcastID)

        let fake = FakeVectorStore()
        let indexer = EpisodeMetadataIndexer(store: fake)
        await indexer.runBackfill(appStore: store)

        let upserts = await fake.upsertCalls
        let chunk = try? XCTUnwrap(upserts.flatMap { $0 }.first)
        XCTAssertEqual(chunk?.episodeID, ep.id)
        XCTAssertEqual(chunk?.podcastID, podcastID)
        XCTAssertEqual(chunk?.startMS, 0)
        XCTAssertEqual(chunk?.endMS, 0)
        XCTAssertNil(chunk?.speakerID)
        let text = chunk?.text ?? ""
        XCTAssertTrue(text.contains("Episode Title"))
        XCTAssertTrue(text.contains("Hello world."), "html should be stripped: got \(text)")
    }

    func testBackfillIsIdempotentAcrossRuns() async {
        let podcastID = UUID()
        let ep = makeEpisode(podcastID: podcastID, title: "Solo", description: "desc")
        store.upsertEpisodes([ep], forPodcast: podcastID)

        let fake = FakeVectorStore()
        let indexer = EpisodeMetadataIndexer(store: fake)

        await indexer.runBackfill(appStore: store)
        await indexer.runBackfill(appStore: store)

        let upserts = await fake.upsertCalls
        XCTAssertEqual(upserts.flatMap { $0 }.count, 1,
                       "a second backfill must skip episodes already flagged by the first")
    }

    // MARK: - Helpers

    private func makeEpisode(
        podcastID: UUID,
        title: String,
        description: String,
        metadataIndexed: Bool = false
    ) -> Episode {
        Episode(
            id: UUID(),
            podcastID: podcastID,
            guid: UUID().uuidString,
            title: title,
            description: description,
            pubDate: Date(),
            enclosureURL: URL(string: "https://example.com/\(UUID().uuidString).mp3")!,
            metadataIndexed: metadataIndexed
        )
    }
}

// MARK: - FakeVectorStore

private actor FakeVectorStore: VectorStore {
    private(set) var upsertCalls: [[Chunk]] = []
    private var shouldFail: Bool = false

    func setShouldFail(_ value: Bool) {
        shouldFail = value
    }

    func upsert(chunks: [Chunk]) async throws {
        if shouldFail {
            throw VectorStoreError.backingStorageFailure("test fake configured to fail")
        }
        upsertCalls.append(chunks)
    }

    func deleteAll(forEpisodeID: UUID) async throws {}

    func topK(
        _ k: Int,
        for queryVector: [Float],
        scope: ChunkScope?
    ) async throws -> [ChunkMatch] { [] }

    func hybridTopK(
        _ k: Int,
        query: String,
        queryVector: [Float],
        scope: ChunkScope?
    ) async throws -> [ChunkMatch] { [] }
}
