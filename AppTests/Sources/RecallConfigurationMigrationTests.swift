import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class RecallConfigurationMigrationTests: XCTestCase {
    func testLegacySwiftSelectionImportsOnceThenSwiftStateIsRetired() throws {
        let fileURL = AppStateTestSupport.uniqueTempFileURL()
        let persistence = Persistence(fileURL: fileURL)
        defer { persistence.reset() }

        var legacy = AppState()
        legacy.settings.legacyRecallEmbeddingsModel = "ollama:qwen3-embedding"
        legacy.settings.legacyRecallEmbeddingsModelName = "Qwen Embedding"
        legacy.settings.legacyRecallRerankerEnabled = true
        XCTAssertTrue(persistence.write(legacy, revision: 1))

        let first = AppStateStore(
            persistence: persistence,
            sharedFeedHost: QueuedCoreFeedHost([]),
            startSubscriptionRefresh: false
        )
        defer { first.sharedLibrary?.shutdown() }

        let imported = try XCTUnwrap(first.recallConfiguration)
        XCTAssertEqual(imported.origin, .legacySwift)
        XCTAssertEqual(imported.embeddingProvider, .ollama)
        XCTAssertEqual(imported.embeddingModel, "qwen3-embedding")
        XCTAssertEqual(imported.storedEmbeddingModelId, "ollama:qwen3-embedding")
        XCTAssertTrue(imported.rerankerEnabled)
        XCTAssertNil(first.state.settings.legacyRecallConfigurationSeed)
        XCTAssertNil(try persistence.load().settings.legacyRecallConfigurationSeed)

        first.sharedLibrary?.shutdown()
        let reopened = try Pod0Facade.open(
            storePath: persistence.sharedCoreStoreURL.path
        )
        guard case .recallConfiguration(let restored) = reopened.snapshot(
            request: ProjectionRequest(
                scope: .recallConfiguration,
                offset: 0,
                maxItems: 1
            )
        ).projection else { return XCTFail("Expected persisted recall configuration") }
        XCTAssertEqual(restored, imported)
    }
}
