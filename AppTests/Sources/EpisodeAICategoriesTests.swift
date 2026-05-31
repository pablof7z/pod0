import XCTest
@testable import Podcastr

/// Coverage for #45 — AI category labels flowing from the Rust kernel
/// projection onto the `Episode` domain model and surviving Codable
/// persistence.
///
/// Two seams are pinned:
///   1. `EpisodeSummary` (the wire type) decodes `aiCategories` from the
///      snapshot JSON — and tolerates the key being absent (Rust omits it
///      when empty per the D5 omit-on-empty convention).
///   2. `Episode` round-trips `aiCategories` through Codable, encoding the
///      key only when non-empty so old persisted records (which never had
///      the key) still decode cleanly to an empty array.
final class EpisodeAICategoriesTests: XCTestCase {

    // MARK: - Wire type (EpisodeSummary) decode

    func testEpisodeSummaryDecodesAICategories() throws {
        let json = """
        {
            "id": "11111111-1111-1111-1111-111111111111",
            "title": "Metabolic flexibility",
            "aiCategories": ["Health", "Science"]
        }
        """.data(using: .utf8)!
        let summary = try JSONDecoder().decode(EpisodeSummary.self, from: json)
        XCTAssertEqual(summary.aiCategories, ["Health", "Science"])
    }

    func testEpisodeSummaryAbsentAICategoriesDecodesEmpty() throws {
        // Rust omits the key when the vec is empty (D5). The wrapped default
        // must decode that to `[]`, not fail.
        let json = """
        {
            "id": "22222222-2222-2222-2222-222222222222",
            "title": "No categories yet"
        }
        """.data(using: .utf8)!
        let summary = try JSONDecoder().decode(EpisodeSummary.self, from: json)
        XCTAssertTrue(summary.aiCategories.isEmpty)
    }

    // MARK: - Domain model (Episode) Codable migration

    func testEpisodeRoundTripsAICategories() throws {
        let episode = makeEpisode(aiCategories: ["Technology", "Business"])
        let data = try JSONEncoder().encode(episode)
        let decoded = try JSONDecoder().decode(Episode.self, from: data)
        XCTAssertEqual(decoded.aiCategories, ["Technology", "Business"])
    }

    func testEpisodeOmitsEmptyAICategoriesOnEncode() throws {
        let episode = makeEpisode(aiCategories: [])
        let data = try JSONEncoder().encode(episode)
        let object = try XCTUnwrap(
            try JSONSerialization.jsonObject(with: data) as? [String: Any]
        )
        XCTAssertNil(
            object["aiCategories"],
            "Empty aiCategories must be omitted from the encoded payload so the field stays absent for untagged episodes."
        )
    }

    func testEpisodeDecodesLegacyRecordWithoutAICategories() throws {
        // A record persisted before #45 has no `aiCategories` key. It must
        // decode to an empty array rather than throwing.
        let episode = makeEpisode(aiCategories: ["Will be stripped"])
        var object = try XCTUnwrap(
            try JSONSerialization.jsonObject(
                with: try JSONEncoder().encode(episode)
            ) as? [String: Any]
        )
        object.removeValue(forKey: "aiCategories")
        let legacyData = try JSONSerialization.data(withJSONObject: object)
        let decoded = try JSONDecoder().decode(Episode.self, from: legacyData)
        XCTAssertTrue(decoded.aiCategories.isEmpty)
    }

    // MARK: - Helpers

    private func makeEpisode(aiCategories: [String]) -> Episode {
        Episode(
            podcastID: UUID(),
            guid: "guid-\(UUID().uuidString)",
            title: "Test episode",
            pubDate: Date(timeIntervalSince1970: 1_700_000_000),
            enclosureURL: URL(string: "https://example.com/audio.mp3")!,
            aiCategories: aiCategories
        )
    }
}
