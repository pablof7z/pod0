import XCTest
@testable import Podcastr

final class LocalEmbeddingsClientTests: XCTestCase {

    // MARK: Stubs

    /// Deterministic local provider whose readiness/dimensions/output are
    /// injectable so we can exercise every branch of the fallback ladder.
    private struct StubProvider: EmbeddingProvider {
        let dimensions: Int
        let isReady: Bool
        var failWith: Error?
        var marker: Float = 1

        func embed(_ texts: [String]) async throws -> [[Float]] {
            if let failWith { throw failWith }
            return texts.map { _ in Array(repeating: marker, count: dimensions) }
        }
    }

    /// Cloud client that records whether it was called and tags its output.
    private final class SpyCloud: EmbeddingsClient, @unchecked Sendable {
        let dimensions: Int
        private(set) var calls = 0
        init(dimensions: Int) { self.dimensions = dimensions }
        func embed(_ texts: [String]) async throws -> [[Float]] {
            calls += 1
            return texts.map { _ in Array(repeating: 9, count: dimensions) }
        }
    }

    // MARK: Tests

    func testUsesLocalWhenReadyAndDimensionsMatch() async throws {
        let provider = StubProvider(dimensions: 384, isReady: true, marker: 1)
        let cloud = SpyCloud(dimensions: 384)
        let client = LocalEmbeddingsClient(provider: provider, cloud: cloud, indexDimensions: 384)

        XCTAssertTrue(client.prefersLocal)
        let vectors = try await client.embed(["a", "b"])
        XCTAssertEqual(cloud.calls, 0, "cloud should not be touched on the local path")
        XCTAssertEqual(vectors.count, 2)
        XCTAssertEqual(vectors[0].first, 1, "vectors should come from the local provider")
    }

    func testFallsBackToCloudWhenDimensionsMismatchEvenIfReady() async throws {
        // The load-bearing safety guard: a 384-dim model must NOT feed a
        // 1024-dim index, even when downloaded and ready.
        let provider = StubProvider(dimensions: 384, isReady: true)
        let cloud = SpyCloud(dimensions: 1024)
        let client = LocalEmbeddingsClient(provider: provider, cloud: cloud, indexDimensions: 1024)

        XCTAssertFalse(client.prefersLocal)
        let vectors = try await client.embed(["a"])
        XCTAssertEqual(cloud.calls, 1)
        XCTAssertEqual(vectors[0].count, 1024)
        XCTAssertEqual(vectors[0].first, 9, "vectors should come from the cloud client")
    }

    func testFallsBackToCloudWhenModelNotReady() async throws {
        let provider = StubProvider(dimensions: 384, isReady: false)
        let cloud = SpyCloud(dimensions: 384)
        let client = LocalEmbeddingsClient(provider: provider, cloud: cloud, indexDimensions: 384)

        XCTAssertFalse(client.prefersLocal)
        let vectors = try await client.embed(["a"])
        XCTAssertEqual(cloud.calls, 1)
        XCTAssertEqual(vectors[0].first, 9)
    }

    func testFallsBackToCloudOnLocalInferenceFailure() async throws {
        let provider = StubProvider(
            dimensions: 384,
            isReady: true,
            failWith: EmbeddingProviderError.inferenceFailed(detail: "boom")
        )
        let cloud = SpyCloud(dimensions: 384)
        let client = LocalEmbeddingsClient(provider: provider, cloud: cloud, indexDimensions: 384)

        let vectors = try await client.embed(["a"])
        XCTAssertEqual(cloud.calls, 1, "an on-device failure should degrade to cloud, not fail ingest")
        XCTAssertEqual(vectors[0].first, 9)
    }

    func testEmptyInputShortCircuits() async throws {
        let provider = StubProvider(dimensions: 384, isReady: true)
        let cloud = SpyCloud(dimensions: 384)
        let client = LocalEmbeddingsClient(provider: provider, cloud: cloud, indexDimensions: 384)

        let vectors = try await client.embed([])
        XCTAssertTrue(vectors.isEmpty)
        XCTAssertEqual(cloud.calls, 0)
    }
}
