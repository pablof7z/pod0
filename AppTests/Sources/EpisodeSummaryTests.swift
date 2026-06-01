import XCTest
@testable import Podcastr

/// Coverage for the AI episode summary flowing from the Rust kernel projection
/// (`EpisodeSummary.summary`) onto the `Episode` domain model and surviving
/// Codable persistence — the replacement for the deleted Swift
/// `LiveEpisodeSummarizerAdapter`.
///
/// Pins the seams that the `aiCategories` "decoded then dropped in toEpisode"
/// bug taught us to guard:
///   1. The wire type decodes `summary`, tolerating an absent key (D5).
///   2. `toEpisode` carries `summary` onto the domain model (the carry-through
///      that silently regresses if a mapping line is dropped).
///   3. `Episode` round-trips `summary` through Codable, omitting the key when
///      `nil` so pre-summary records still decode.
final class EpisodeSummaryTests: XCTestCase {

    private let episodeUUID = "11111111-1111-1111-1111-111111111111"

    // MARK: - Wire type (EpisodeSummary) decode

    func testEpisodeSummaryDecodesSummary() throws {
        let json = """
        {
            "id": "\(episodeUUID)",
            "title": "Metabolic flexibility",
            "summary": "A concise two sentence summary."
        }
        """.data(using: .utf8)!
        let summary = try JSONDecoder().decode(EpisodeSummary.self, from: json)
        XCTAssertEqual(summary.summary, "A concise two sentence summary.")
    }

    func testEpisodeSummaryAbsentSummaryDecodesNil() throws {
        // Rust omits the key when `None` (D5). Decoding must yield nil, not fail.
        let json = """
        { "id": "\(episodeUUID)", "title": "No summary yet" }
        """.data(using: .utf8)!
        let summary = try JSONDecoder().decode(EpisodeSummary.self, from: json)
        XCTAssertNil(summary.summary)
    }

    // NOTE: the `EpisodeSummary.toEpisode` carry-through is `fileprivate`, so it
    // cannot be invoked from tests (same constraint as EpisodeAICategoriesTests).
    // The `summary: summary` mapping line is compiler-guarded — `toEpisode`
    // builds the domain model with an explicit `Episode(...)` initializer, which
    // would fail to compile if the field were dropped. The wire-decode and
    // Episode-Codable tests here bracket that mapping on both sides.

    // MARK: - Domain model (Episode) Codable migration

    func testEpisodeRoundTripsSummary() throws {
        let episode = makeEpisode(summary: "Persisted summary.")
        let data = try JSONEncoder().encode(episode)
        let decoded = try JSONDecoder().decode(Episode.self, from: data)
        XCTAssertEqual(decoded.summary, "Persisted summary.")
    }

    func testEpisodeOmitsNilSummaryOnEncode() throws {
        let episode = makeEpisode(summary: nil)
        let object = try XCTUnwrap(
            try JSONSerialization.jsonObject(
                with: try JSONEncoder().encode(episode)
            ) as? [String: Any]
        )
        XCTAssertNil(object["summary"])
    }

    func testEpisodeDecodesLegacyRecordWithoutSummary() throws {
        let episode = makeEpisode(summary: "Will be stripped")
        var object = try XCTUnwrap(
            try JSONSerialization.jsonObject(
                with: try JSONEncoder().encode(episode)
            ) as? [String: Any]
        )
        object.removeValue(forKey: "summary")
        let legacyData = try JSONSerialization.data(withJSONObject: object)
        let decoded = try JSONDecoder().decode(Episode.self, from: legacyData)
        XCTAssertNil(decoded.summary)
    }

    // MARK: - Helpers

    private func makeEpisode(summary: String?) -> Episode {
        Episode(
            podcastID: UUID(),
            guid: "guid-\(UUID().uuidString)",
            title: "Test episode",
            pubDate: Date(timeIntervalSince1970: 1_700_000_000),
            enclosureURL: URL(string: "https://example.com/audio.mp3")!,
            summary: summary
        )
    }
}
