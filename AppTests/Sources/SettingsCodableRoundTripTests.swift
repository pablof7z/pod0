import XCTest
@testable import Podcastr

/// Regression coverage for the on-disk `Settings` JSON round-trip. The
/// custom `CodingKeys` enum + manual encode/decode pair makes it easy
/// for a newly-added field to be silently dropped — the type compiles
/// and the in-memory toggle works, but the value never persists. These
/// tests exercise every Bool toggle through encode → decode so a
/// missing key surfaces immediately.
///
/// Background: `autoPlayNext` and `autoDeleteDownloadsAfterPlayed` were
/// missing from `CodingKeys` for a while — the toggle worked in-session
/// but reset on every relaunch. The fix added them; this lock keeps
/// future additions honest.
final class SettingsCodableRoundTripTests: XCTestCase {

    private func roundTrip(_ s: Settings) throws -> Settings {
        let data = try JSONEncoder().encode(s)
        return try JSONDecoder().decode(Settings.self, from: data)
    }

    // MARK: - Each toggle survives flip + round-trip

    func testAutoMarkPlayedAtEndPersists() throws {
        var s = Settings()
        s.autoMarkPlayedAtEnd = false  // default true
        let restored = try roundTrip(s)
        XCTAssertEqual(restored.autoMarkPlayedAtEnd, false)
    }

    func testAutoDeleteDownloadsAfterPlayedPersists() throws {
        var s = Settings()
        s.autoDeleteDownloadsAfterPlayed = true  // default false
        let restored = try roundTrip(s)
        XCTAssertEqual(restored.autoDeleteDownloadsAfterPlayed, true)
    }

    func testAutoPlayNextPersists() throws {
        var s = Settings()
        s.autoPlayNext = false  // default true
        let restored = try roundTrip(s)
        XCTAssertEqual(restored.autoPlayNext, false)
    }

    func testAutoIngestPublisherTranscriptsPersists() throws {
        var s = Settings()
        s.autoIngestPublisherTranscripts = false  // default true
        let restored = try roundTrip(s)
        XCTAssertEqual(restored.autoIngestPublisherTranscripts, false)
    }

    func testAutoFallbackToScribePersists() throws {
        var s = Settings()
        s.autoFallbackToScribe = false  // default true
        let restored = try roundTrip(s)
        XCTAssertEqual(restored.autoFallbackToScribe, false)
    }

    func testAssemblyAISpeechSettingsPersist() throws {
        var s = Settings()
        s.sttProvider = .assemblyAI
        s.assemblyAISTTModel = "universal-3-pro"
        let restored = try roundTrip(s)
        XCTAssertEqual(restored.sttProvider, .assemblyAI)
        XCTAssertEqual(restored.assemblyAISTTModel, "universal-3-pro")
    }

    // MARK: - Forward compatibility

    func testDecodingMissingKeyFallsBackToDefault() throws {
        // An older `Settings` JSON written before a field existed must
        // decode without throwing — the new field gets its default. This
        // is the exact migration shape every new toggle relies on.
        let json = #"{"defaultPlaybackRate": 1.5}"#.data(using: .utf8)!
        let decoded = try JSONDecoder().decode(Settings.self, from: json)
        XCTAssertEqual(decoded.defaultPlaybackRate, 1.5)
        XCTAssertEqual(decoded.autoPlayNext, true, "Missing key should fall back to property default")
        XCTAssertEqual(decoded.autoDeleteDownloadsAfterPlayed, false)
        XCTAssertEqual(decoded.autoIngestPublisherTranscripts, true)
    }
}
