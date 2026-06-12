import XCTest
@testable import Podcastr

/// Cross-language wire-contract guard for chapters and transcript entries
/// embedded in `PodcastUpdate`.
///
/// WHY THIS EXISTS: `ChapterSummary.start_secs` and `TranscriptEntry.start_secs`
/// are required `f64` fields on the Rust side. If a NaN propagates into them
/// (e.g. `ai_chapters::stub_chapters` dividing by a NaN episode duration), serde
/// serialises them as JSON `null`.  The bridge decoder (`keyDecodingStrategy =
/// .convertFromSnakeCase`) then throws `keyNotFound` for the Swift required
/// `Double` and drops the **entire** `PodcastUpdate` frame — the #371-class
/// failure, but remotely triggerable from any RSS feed.
///
/// This test decodes `tests/fixtures/podcast_update_with_chapters.json` — a
/// **Rust-emitted** frame with three chapters and two transcript entries —
/// through the **exact** bridge seam (`KernelDecoding.decodePodcastUpdate`).
/// Any schema drift (wrong field name, accidental CodingKeys, non-Option field
/// going null) fails CI instead of freezing the app.
final class PodcastUpdateChapterDecodeTests: XCTestCase {

    private func loadFixture() throws -> Data {
        let bundle = Bundle(for: Self.self)
        guard let url = bundle.url(
            forResource: "podcast_update_with_chapters", withExtension: "json")
        else {
            XCTFail(
                "podcast_update_with_chapters.json missing from test bundle — "
                    + "check Project.swift resources under AppTests"
            )
            return Data()
        }
        return try Data(contentsOf: url)
    }

    // MARK: - Frame-level decode

    /// The whole frame decodes through the bridge seam — chapters must NOT
    /// torpedo the decode (the failure mode this guards).
    func testRustChaptersFixtureDecodesThroughBridgeSeam() throws {
        let data = try loadFixture()
        let update = try KernelDecoding.decodePodcastUpdate(from: data)

        // Library survives with the expected shape.
        XCTAssertEqual(update.library.count, 1)
        XCTAssertEqual(update.library.first?.title, "Science Podcast")

        let ep = try XCTUnwrap(
            update.library.first?.episodes.first,
            "Episode must decode — nil means the library or episodes array dropped"
        )
        XCTAssertEqual(ep.title, "Deep Dive into Metabolic Flexibility")
    }

    // MARK: - Chapters

    /// Three chapters survive round-trip through the bridge decoder, with all
    /// optional fields (image_url, url, is_ai_generated) decoded correctly.
    func testChaptersSurviveBridgeDecode() throws {
        let data = try loadFixture()
        let update = try KernelDecoding.decodePodcastUpdate(from: data)
        let ep = try XCTUnwrap(update.library.first?.episodes.first)
        let chapters = try XCTUnwrap(
            ep.chapters,
            "chapters must decode — nil means the field dropped (keyNotFound or null)"
        )
        XCTAssertEqual(chapters.count, 3, "all three chapters must survive")

        // Chapter 0: Introduction
        let ch0 = chapters[0]
        XCTAssertEqual(ch0.startSecs, 0.0,
                       "ch0.startSecs must be 0.0 — if NaN serialised as null, startSecs would throw")
        XCTAssertEqual(ch0.endSecs, 300.0)
        XCTAssertEqual(ch0.title, "Introduction")
        XCTAssertNil(ch0.imageUrl)
        XCTAssertFalse(ch0.isAiGenerated)

        // Chapter 1: named chapter with image + url
        let ch1 = chapters[1]
        XCTAssertEqual(ch1.startSecs, 300.0)
        XCTAssertEqual(ch1.endSecs, 1200.0)
        XCTAssertEqual(ch1.title, "What is Metabolic Flexibility?")
        XCTAssertEqual(ch1.imageUrl, "https://ex.com/ch2.png")
        XCTAssertEqual(ch1.url, "https://ex.com/notes#2")
        XCTAssertFalse(ch1.isAiGenerated)

        // Chapter 2: AI-generated, no image/url
        let ch2 = chapters[2]
        XCTAssertEqual(ch2.startSecs, 1200.0)
        XCTAssertEqual(ch2.endSecs, 3600.0)
        XCTAssertEqual(ch2.title, "AI-Generated Deep Dive")
        XCTAssertTrue(ch2.isAiGenerated)
        XCTAssertNil(ch2.imageUrl)
    }

    // MARK: - Transcript entries

    /// Two transcript entries survive, including the one with optional fields
    /// omitted (end_secs / speaker missing from the Rust wire).
    func testTranscriptEntriesSurviveBridgeDecode() throws {
        let data = try loadFixture()
        let update = try KernelDecoding.decodePodcastUpdate(from: data)
        let ep = try XCTUnwrap(update.library.first?.episodes.first)
        let entries = try XCTUnwrap(
            ep.transcriptEntries,
            "transcriptEntries must decode — nil means the field dropped"
        )
        XCTAssertEqual(entries.count, 2)

        // Entry 0: all optional fields present
        let e0 = entries[0]
        XCTAssertEqual(e0.startSecs, 0.0)
        XCTAssertEqual(e0.endSecs, 15.0)
        XCTAssertEqual(e0.speaker, "Host")
        XCTAssertEqual(e0.text, "Welcome back to the show.")

        // Entry 1: end_secs and speaker absent on the Rust wire (D5 omit)
        let e1 = entries[1]
        XCTAssertEqual(e1.startSecs, 15.0)
        XCTAssertNil(e1.endSecs)
        XCTAssertNil(e1.speaker)
        XCTAssertEqual(e1.text, "Today we explore metabolic flexibility.")
    }

    // MARK: - Widget

    /// The widget embedded alongside chapters also decodes correctly.
    func testWidgetCoexistsWithChapters() throws {
        let data = try loadFixture()
        let update = try KernelDecoding.decodePodcastUpdate(from: data)
        let widget = try XCTUnwrap(update.widget, "widget must decode alongside chapters")
        XCTAssertTrue(widget.isPlaying)
        XCTAssertEqual(widget.durationSecs, 3600.0)
        XCTAssertEqual(widget.positionSecs, 600.0)
    }

    // MARK: - Plain-decoder failure contract

    /// Pin the failure mode: a plain (non-.convertFromSnakeCase) decoder must
    /// fail, proving the bridge config is load-bearing for this fixture too.
    func testPlainDecoderFailsOnChaptersFixture() throws {
        let data = try loadFixture()
        XCTAssertThrowsError(
            try JSONDecoder().decode(PodcastUpdate.self, from: data),
            "A plain decoder must fail on the snake_case Rust frame"
        )
    }
}
