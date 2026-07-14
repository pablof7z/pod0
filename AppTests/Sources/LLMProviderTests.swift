import XCTest
@testable import Podcastr

final class LLMProviderTests: XCTestCase {
    func testPlainModelIDsRemainOpenRouter() {
        let reference = LLMModelReference(storedID: "openai/gpt-4o-mini")

        XCTAssertEqual(reference.provider, .openRouter)
        XCTAssertEqual(reference.modelID, "openai/gpt-4o-mini")
        XCTAssertEqual(reference.storedID, "openai/gpt-4o-mini")
    }

    func testOllamaModelIDsRoundTripWithProviderPrefix() {
        let reference = LLMModelReference(storedID: "ollama:gpt-oss:120b")

        XCTAssertEqual(reference.provider, .ollama)
        XCTAssertEqual(reference.modelID, "gpt-oss:120b")
        XCTAssertEqual(reference.storedID, "ollama:gpt-oss:120b")
    }

    func testSettingsPersistOllamaBYOKMetadataAndEmbeddingSelection() throws {
        var settings = Settings()
        settings.markOllamaBYOK(
            keyID: "key_ollama",
            keyLabel: "Podcast Ollama",
            connectedAt: Date(timeIntervalSince1970: 1_700_000_000)
        )
        settings.embeddingsModel = "ollama:qwen3-embedding"
        settings.embeddingsModelName = "qwen3-embedding"

        let data = try JSONEncoder().encode(settings)
        let decoded = try JSONDecoder().decode(Settings.self, from: data)

        XCTAssertEqual(decoded.ollamaCredentialSource, .byok)
        XCTAssertEqual(decoded.ollamaBYOKKeyID, "key_ollama")
        XCTAssertEqual(decoded.ollamaBYOKKeyLabel, "Podcast Ollama")
        XCTAssertEqual(decoded.embeddingsModel, "ollama:qwen3-embedding")
        XCTAssertEqual(decoded.embeddingsModelName, "qwen3-embedding")
    }
}
