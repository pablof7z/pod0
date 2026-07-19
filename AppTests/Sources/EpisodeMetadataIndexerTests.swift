import XCTest
@testable import Podcastr

@MainActor
final class EpisodeMetadataIndexerTests: XCTestCase {
    private var fileURL: URL!
    private var appStore: AppStateStore!

    override func setUp() async throws {
        try await super.setUp()
        let made = AppStateTestSupport.makeIsolatedStore()
        appStore = made.store
        fileURL = made.fileURL
    }

    override func tearDown() async throws {
        if let fileURL { AppStateTestSupport.disposeIsolatedStore(at: fileURL) }
        appStore = nil
        fileURL = nil
        try await super.tearDown()
    }

    func testExecutorBuildsMetadataChunkWithoutMutatingEpisodeWorkflowFlags() async throws {
        let episode = makeEpisode(
            title: "Episode Title", description: "<p>Hello <b>world</b>.</p>"
        )
        appStore.installEpisodeFixtures([episode], forPodcast: episode.podcastID)
        let fake = FakeVectorStore()

        try await EpisodeMetadataIndexer(store: fake).indexEpisode(
            id: episode.id, appStore: appStore
        )

        let calls = await fake.snapshot()
        let chunk = try XCTUnwrap(calls.first?.first)
        XCTAssertEqual(chunk.episodeID, episode.id)
        XCTAssertEqual(chunk.podcastID, episode.podcastID)
        XCTAssertEqual(chunk.text, "Episode Title\n\nHello world.")
        XCTAssertEqual(appStore.episode(id: episode.id), episode)
    }

    func testRepositoryFailureEscapesForCoordinatorClassification() async throws {
        let episode = makeEpisode(title: "Failure", description: "Body")
        appStore.installEpisodeFixtures([episode], forPodcast: episode.podcastID)
        let fake = FakeVectorStore()
        await fake.setShouldFail(true)

        do {
            try await EpisodeMetadataIndexer(store: fake).indexEpisode(
                id: episode.id, appStore: appStore
            )
            XCTFail("Expected vector repository failure")
        } catch let error as VectorStoreError {
            guard case .backingStorageFailure = error else {
                return XCTFail("Unexpected error: \(error)")
            }
        }
    }

    func testEmptyMetadataProducesNoVectorWrite() async throws {
        let episode = makeEpisode(title: "", description: "  \n ")
        appStore.installEpisodeFixtures([episode], forPodcast: episode.podcastID)
        let fake = FakeVectorStore()

        try await EpisodeMetadataIndexer(store: fake).indexEpisode(
            id: episode.id, appStore: appStore
        )

        let calls = await fake.snapshot()
        XCTAssertTrue(calls.isEmpty)
    }

    private func makeEpisode(title: String, description: String) -> Episode {
        Episode(
            podcastID: UUID(), guid: UUID().uuidString,
            title: title, description: description, pubDate: Date(),
            enclosureURL: URL(string: "https://example.com/\(UUID().uuidString).mp3")!
        )
    }
}

private actor FakeVectorStore: VectorStore {
    private(set) var upsertCalls: [[Chunk]] = []
    private var shouldFail = false

    func setShouldFail(_ value: Bool) { shouldFail = value }

    func snapshot() -> [[Chunk]] { upsertCalls }

    func upsert(chunks: [Chunk]) async throws {
        if shouldFail { throw VectorStoreError.backingStorageFailure("injected") }
        upsertCalls.append(chunks)
    }

    func deleteAll(forEpisodeID: UUID) async throws {}

    func topK(_ k: Int, for queryVector: [Float], scope: ChunkScope?) async throws -> [ChunkMatch] { [] }

    func hybridTopK(
        _ k: Int, query: String, queryVector: [Float], scope: ChunkScope?
    ) async throws -> [ChunkMatch] { [] }
}
