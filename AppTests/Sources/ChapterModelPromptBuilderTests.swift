import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

final class ChapterModelPromptBuilderTests: XCTestCase {
    func testEnrichmentUsesTypedPublisherProjectionWithoutEpisodeChapterFallback() {
        let episode = Episode(
            podcastID: UUID(),
            guid: "typed-publisher",
            title: "Typed Publisher Episode",
            pubDate: Date(),
            enclosureURL: URL(string: "https://example.com/episode.mp3")!
        )
        XCTAssertNil(episode.chapters)
        let prompt = ChapterModelPromptBuilder.make(
            episode: episode,
            transcript: transcript(episodeID: episode.id),
            publisherChapters: [ChapterInput(
                startMilliseconds: 12_000,
                endMilliseconds: 45_000,
                title: "Typed publisher boundary",
                summary: nil,
                imageUrl: nil,
                linkUrl: nil,
                includeInTableOfContents: true,
                sourceEpisodeId: nil
            )]
        )

        XCTAssertTrue(prompt.system.contains("publisher chapter boundaries"))
        XCTAssertTrue(prompt.user.contains("[0] 12s — Typed publisher boundary"))
        XCTAssertTrue(prompt.user.contains("use these exact indices"))
    }

    func testPromptBoundsTranscriptBeforeNativeModelExecution() {
        let episode = Episode(
            podcastID: UUID(),
            guid: "bounded-prompt",
            title: "Bounded Prompt",
            pubDate: Date(),
            enclosureURL: URL(string: "https://example.com/episode.mp3")!
        )
        let longText = String(
            repeating: "x",
            count: ChapterModelPromptBuilder.maximumTranscriptCharacters * 2
        )
        let transcript = Transcript(
            episodeID: episode.id,
            language: "en",
            source: .publisher,
            segments: [Segment(start: 0, end: 10, text: longText)]
        )
        let prompt = ChapterModelPromptBuilder.make(
            episode: episode,
            transcript: transcript,
            publisherChapters: nil
        )

        XCTAssertLessThanOrEqual(
            prompt.user.count,
            ChapterModelPromptBuilder.maximumTranscriptCharacters + 100
        )
        XCTAssertFalse(prompt.user.contains(String(repeating: "x", count: 28_001)))
    }

    private func transcript(episodeID: UUID) -> Transcript {
        Transcript(
            episodeID: episodeID,
            language: "en",
            source: .publisher,
            segments: [Segment(start: 0, end: 10, text: "Publisher transcript")]
        )
    }
}
