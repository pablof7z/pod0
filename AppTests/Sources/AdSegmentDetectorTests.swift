import XCTest
@testable import Podcastr

/// Pins the pure pieces of `AIChapterCompiler`'s ad-segment + chapter
/// pipeline. The LLM round-trip itself isn't tested here — it requires a
/// real OpenRouter / Ollama key. Anything past the parse boundary is
/// integration territory.
///
/// Historical note: this file used to test `AdSegmentDetector`. That
/// service was folded into `AIChapterCompiler` so chapters, summaries, and
/// ads are produced in a single LLM call — same parsing contract, now
/// shared with the chapter path.
@MainActor
final class AdSegmentDetectorTests: XCTestCase {

    private var compiler: AIChapterCompiler!

    override func setUp() {
        super.setUp()
        compiler = AIChapterCompiler.shared
    }

    override func tearDown() {
        compiler = nil
        super.tearDown()
    }

    // MARK: - parseEnrichOnly (ads + summaries by index)

    func testParseEnrichOnlyAdsHappyPath() {
        let raw = """
        {"summaries":[],"ads":[{"start":0,"end":30,"kind":"preroll"},{"start":600,"end":660,"kind":"midroll"}]}
        """
        let (_, ads) = compiler.parseEnrichOnly(raw, durationCap: 3600)
        XCTAssertNotNil(ads)
        XCTAssertEqual(ads?.count, 2)
        XCTAssertEqual(ads?[0].kind, .preroll)
        XCTAssertEqual(ads?[0].start, 0)
        XCTAssertEqual(ads?[0].end, 30)
        XCTAssertEqual(ads?[1].kind, .midroll)
    }

    func testParseEnrichOnlyAcceptsLegacyStartEndSecondsFields() {
        // The merged parser tolerates `start_seconds` / `end_seconds` so older
        // prompt outputs still validate.
        let raw = """
        {"ads":[{"start_seconds":0,"end_seconds":30,"kind":"preroll"}]}
        """
        let (_, ads) = compiler.parseEnrichOnly(raw, durationCap: 3600)
        XCTAssertEqual(ads?.count, 1)
        XCTAssertEqual(ads?.first?.start, 0)
        XCTAssertEqual(ads?.first?.end, 30)
    }

    func testParseEnrichOnlyReturnsEmptyForEmptyAdsArray() {
        let raw = """
        {"ads":[]}
        """
        let (_, ads) = compiler.parseEnrichOnly(raw, durationCap: 3600)
        XCTAssertNotNil(ads, "empty array is a valid 'no ads found' result, not nil")
        XCTAssertEqual(ads?.count, 0)
    }

    func testParseEnrichOnlyRejectsMalformedJSON() {
        let (s1, a1) = compiler.parseEnrichOnly("not json", durationCap: 3600)
        XCTAssertTrue(s1.isEmpty)
        XCTAssertNil(a1)
    }

    func testParseEnrichOnlyDropsZeroLengthRanges() {
        let raw = """
        {"ads":[{"start":100,"end":100,"kind":"midroll"},{"start":200,"end":260,"kind":"midroll"}]}
        """
        let (_, ads) = compiler.parseEnrichOnly(raw, durationCap: 3600)
        XCTAssertEqual(ads?.count, 1, "zero-length range should be rejected")
        XCTAssertEqual(ads?.first?.start, 200)
    }

    func testParseEnrichOnlyRejectsOverlappingRanges() {
        let raw = """
        {"ads":[{"start":0,"end":120,"kind":"preroll"},{"start":60,"end":180,"kind":"midroll"}]}
        """
        let (_, ads) = compiler.parseEnrichOnly(raw, durationCap: 3600)
        XCTAssertEqual(ads?.count, 1)
        XCTAssertEqual(ads?.first?.end, 120)
    }

    func testParseEnrichOnlyClampsToDurationCap() {
        let raw = """
        {"ads":[{"start":3500,"end":9999,"kind":"postroll"}]}
        """
        let (_, ads) = compiler.parseEnrichOnly(raw, durationCap: 3600)
        XCTAssertEqual(ads?.count, 1)
        XCTAssertEqual(ads?.first?.end, 3600, "end past duration cap is clamped")
    }

    func testParseEnrichOnlyDefaultsKindToMidroll() {
        let raw = """
        {"ads":[{"start":0,"end":30}]}
        """
        let (_, ads) = compiler.parseEnrichOnly(raw, durationCap: 3600)
        XCTAssertEqual(ads?.first?.kind, .midroll)
    }

    func testParseEnrichOnlySummariesIndexedByChapter() {
        let raw = """
        {"summaries":[{"index":0,"summary":"Intro discussion."},{"index":2,"summary":"Topic B deep dive."}],"ads":[]}
        """
        let (summaries, _) = compiler.parseEnrichOnly(raw, durationCap: 3600)
        XCTAssertEqual(summaries[0], "Intro discussion.")
        XCTAssertEqual(summaries[2], "Topic B deep dive.")
        XCTAssertNil(summaries[1])
    }

    // MARK: - parseFull (chapters + summaries + ads)

    func testParseFullHappyPath() {
        let raw = """
        {"chapters":[
          {"start":0,"title":"Cold open","summary":"Setup."},
          {"start":120,"title":"Topic A","summary":"First topic."},
          {"start":600,"title":"Topic B","summary":"Second topic."},
          {"start":1200,"title":"Wrap","summary":"Closing."}
        ],"ads":[{"start":60,"end":90,"kind":"preroll"}]}
        """
        let result = compiler.parseFull(raw, durationCap: 3600)
        XCTAssertNotNil(result)
        XCTAssertEqual(result?.chapters.count, 4)
        XCTAssertEqual(result?.chapters.first?.summary, "Setup.")
        XCTAssertTrue(result?.chapters.first?.isAIGenerated == true)
        XCTAssertEqual(result?.ads.count, 1)
        XCTAssertEqual(result?.ads.first?.kind, .preroll)
    }

    func testParseFullRejectsTooFewChapters() {
        // Below `minChapters` (4) → unusable.
        let raw = """
        {"chapters":[{"start":0,"title":"Only","summary":"x"}],"ads":[]}
        """
        XCTAssertNil(compiler.parseFull(raw, durationCap: 3600))
    }

    // MARK: - applySummaries

    func testApplySummariesByIndex() {
        let existing = [
            Episode.Chapter(startTime: 0, title: "A"),
            Episode.Chapter(startTime: 100, title: "B"),
            Episode.Chapter(startTime: 200, title: "C")
        ]
        let merged = compiler.applySummaries(to: existing, indexed: [0: "First.", 2: "Third."])
        XCTAssertEqual(merged[0].summary, "First.")
        XCTAssertNil(merged[1].summary)
        XCTAssertEqual(merged[2].summary, "Third.")
        XCTAssertEqual(merged[0].title, "A", "titles untouched")
    }

    // MARK: - Chapter overlap helper

    func testChapterOverlapsAdReturnsFalseForEmptyAds() {
        let chapter = Episode.Chapter(startTime: 0, title: "Cold open")
        XCTAssertFalse(chapter.overlapsAd(in: [chapter], adSegments: []))
    }

    func testChapterOverlapsAdDetectsContainedAd() {
        let chapters = [
            Episode.Chapter(startTime: 0, title: "Cold open"),
            Episode.Chapter(startTime: 600, title: "Topic A"),
            Episode.Chapter(startTime: 1200, title: "Topic B")
        ]
        let ads = [Episode.AdSegment(start: 660, end: 720, kind: .midroll)]
        XCTAssertTrue(chapters[1].overlapsAd(in: chapters, adSegments: ads))
        XCTAssertFalse(chapters[0].overlapsAd(in: chapters, adSegments: ads))
        XCTAssertFalse(chapters[2].overlapsAd(in: chapters, adSegments: ads))
    }

    func testChapterOverlapsAdUsesExplicitEndTime() {
        let chapter = Episode.Chapter(startTime: 0, endTime: 500, title: "Cold open")
        let nextChapter = Episode.Chapter(startTime: 800, title: "Topic A")
        let ads = [Episode.AdSegment(start: 700, end: 750, kind: .midroll)]
        XCTAssertFalse(chapter.overlapsAd(in: [chapter, nextChapter], adSegments: ads))
    }

    func testChapterOverlapsAdLastChapterExtendsToInfinity() {
        let chapters = [
            Episode.Chapter(startTime: 0, title: "Cold open"),
            Episode.Chapter(startTime: 600, title: "Topic A")
        ]
        let ads = [Episode.AdSegment(start: 3500, end: 3580, kind: .postroll)]
        XCTAssertTrue(chapters[1].overlapsAd(in: chapters, adSegments: ads))
    }

    func testChapterOverlapsAdRespectsHalfOpenInterval() {
        let chapters = [
            Episode.Chapter(startTime: 0, title: "Cold open"),
            Episode.Chapter(startTime: 500, title: "Topic A")
        ]
        let ads = [Episode.AdSegment(start: 500, end: 560, kind: .midroll)]
        XCTAssertFalse(chapters[0].overlapsAd(in: chapters, adSegments: ads))
        XCTAssertTrue(chapters[1].overlapsAd(in: chapters, adSegments: ads))
    }

    // MARK: - Codable round-trip

    func testEpisodeAdSegmentsCodableRoundTrip() throws {
        let original = Episode(
            podcastID: UUID(),
            guid: "rt-1",
            title: "Round-trip",
            pubDate: Date(timeIntervalSince1970: 1_700_000_000),
            enclosureURL: URL(string: "https://example.com/a.mp3")!,
            adSegments: [
                Episode.AdSegment(start: 0, end: 30, kind: .preroll),
                Episode.AdSegment(start: 1200, end: 1260, kind: .midroll)
            ]
        )
        let encoded = try JSONEncoder().encode(original)
        let decoded = try JSONDecoder().decode(Episode.self, from: encoded)
        XCTAssertEqual(decoded.adSegments?.count, 2)
        XCTAssertEqual(decoded.adSegments?.first?.kind, .preroll)
        XCTAssertEqual(decoded.adSegments?.last?.end, 1260)
    }

    func testEpisodeAdSegmentsAbsentFieldDecodesAsNil() throws {
        let template = Episode(
            podcastID: UUID(),
            guid: "legacy",
            title: "Legacy",
            pubDate: Date(timeIntervalSince1970: 1_700_000_000),
            enclosureURL: URL(string: "https://example.com/a.mp3")!,
            adSegments: nil
        )
        let encoded = try JSONEncoder().encode(template)
        guard var dict = try JSONSerialization.jsonObject(with: encoded) as? [String: Any] else {
            return XCTFail("expected dict-shaped Episode encoding")
        }
        dict.removeValue(forKey: "adSegments")
        let strippedData = try JSONSerialization.data(withJSONObject: dict)
        let decoded = try JSONDecoder().decode(Episode.self, from: strippedData)
        XCTAssertNil(decoded.adSegments)
    }

    // MARK: - setEpisodeAdSegments round-trip

    func testSetEpisodeAdSegmentsRoundTripsThroughStore() {
        let result = AppStateTestSupport.makeIsolatedStore()
        defer { AppStateTestSupport.disposeIsolatedStore(at: result.fileURL) }
        let store = result.store

        let episode = Episode(
            podcastID: UUID(),
            guid: "store-rt-1",
            title: "Store Round-trip",
            pubDate: Date(timeIntervalSince1970: 1_700_000_000),
            enclosureURL: URL(string: "https://example.com/a.mp3")!
        )
        store.state.episodes = [episode]

        let segments: [Episode.AdSegment] = [
            Episode.AdSegment(start: 0, end: 30, kind: .preroll),
            Episode.AdSegment(start: 1500, end: 1560, kind: .midroll)
        ]
        store.setEpisodeAdSegments(episode.id, segments: segments)

        let resolved = store.episode(id: episode.id)?.adSegments
        XCTAssertEqual(resolved?.count, 2)
        XCTAssertEqual(resolved?.first?.kind, .preroll)
        XCTAssertEqual(resolved?.last?.start, 1500)

        store.setEpisodeAdSegments(episode.id, segments: [])
        XCTAssertEqual(store.episode(id: episode.id)?.adSegments?.count, 0)
        XCTAssertNotNil(store.episode(id: episode.id)?.adSegments, "empty != nil")
    }
}
