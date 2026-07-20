import CryptoKit
import Pod0Core
import XCTest

final class ChapterObservationBindingTests: XCTestCase {
    func testSwiftQualifiesPublisherModelAndAgentObservationsAsState() {
        let episodeID = EpisodeId(high: 11, low: 101)
        let podcastID = PodcastId(high: 12, low: 101)
        let observedAt = UnixTimestampMilliseconds(value: 1_721_322_123_456)
        let publisherData = Data(#"{"version":"1.2.0","chapters":[{"startTime":0,"title":"Publisher opening"},{"startTime":50,"title":"Publisher source","toc":false}]}"#.utf8)
        let publisher = qualifyPublisherChapterObservation(observation: PublisherChapterObservation(
            episodeId: episodeID,
            podcastId: podcastID,
            resolvedSourceUrl: "https://example.test/chapters.json",
            contentType: "application/json",
            payloadDigest: Self.digest(publisherData),
            payload: publisherData,
            generatedAt: observedAt,
            durationMilliseconds: 100_000
        ))
        guard case let .qualified(publisherArtifact, _) = publisher else {
            return XCTFail("Publisher observation was rejected: \(publisher)")
        }
        XCTAssertEqual(publisherArtifact.provenance.source, .publisher)
        XCTAssertFalse(publisherArtifact.chapters[1].includeInTableOfContents)

        let transcriptDigest = Self.digest(Data("selected transcript".utf8))
        let completion = #"{"chapters":[{"start":5,"title":"Generated one","summary":"One"},{"start":25,"title":"Generated two"},{"start":50,"title":"Generated three"},{"start":75,"title":"Generated four"}],"ads":[]}"#
        func model(_ body: String, mode: ChapterModelObservationMode) -> ModelChapterObservation {
            ModelChapterObservation(
                episodeId: episodeID,
                podcastId: podcastID,
                formatVersion: 1,
                requestedTranscriptVersionId: TranscriptVersionId(high: 13, low: 101),
                requestedTranscriptContentDigest: transcriptDigest,
                selectedTranscriptVersionId: TranscriptVersionId(high: 13, low: 101),
                selectedTranscriptContentDigest: transcriptDigest,
                policyVersion: 1,
                provider: "openrouter",
                model: "fixture-model-v1",
                completionDigest: Self.digest(Data(body.utf8)),
                completion: body,
                generatedAt: observedAt,
                durationMilliseconds: 100_000,
                mode: mode
            )
        }

        let generated = qualifyModelChapterObservation(observation: model(completion, mode: .generate))
        guard case let .qualified(generatedArtifact, _) = generated else {
            return XCTFail("Generated observation was rejected: \(generated)")
        }
        XCTAssertEqual(generatedArtifact.provenance.source, .generated)
        XCTAssertEqual(generatedArtifact.chapters.first?.startMilliseconds, 0)

        let enrichment = #"{"summaries":[{"index":0,"summary":"Enriched"}],"ads":[]}"#
        let enriched = qualifyModelChapterObservation(observation: model(
            enrichment,
            mode: .enrich(publisherArtifact: publisherArtifact)
        ))
        guard case let .qualified(enrichedArtifact, _) = enriched else {
            return XCTFail("Enrichment observation was rejected: \(enriched)")
        }
        XCTAssertEqual(enrichedArtifact.provenance.source, .publisherEnriched)
        XCTAssertEqual(enrichedArtifact.chapters.first?.summary, "Enriched")

        let agent = qualifyAgentComposedChapterObservation(
            observation: AgentComposedChapterObservation(
                episodeId: episodeID,
                podcastId: podcastID,
                compositionRevision: "agent-composition-v1",
                policyVersion: 1,
                provider: "elevenlabs",
                model: "eleven-multilingual-v2",
                sourcePayloadDigest: Self.digest(Data("ordered turns".utf8)),
                generatedAt: observedAt,
                durationMilliseconds: 30_000,
                items: [
                    AgentComposedChapterItem(
                        startSeconds: 0,
                        endSeconds: 10.25,
                        title: "Synthesis",
                        summary: nil,
                        imageUrl: nil,
                        linkUrl: nil,
                        includeInTableOfContents: true,
                        sourceEpisodeId: nil
                    ),
                    AgentComposedChapterItem(
                        startSeconds: 10.25,
                        endSeconds: 30,
                        title: "Source moment",
                        summary: nil,
                        imageUrl: nil,
                        linkUrl: "https://example.test/source?t=42",
                        includeInTableOfContents: true,
                        sourceEpisodeId: EpisodeId(high: 99, low: 7)
                    ),
                ]
            )
        )
        guard case let .qualified(agentArtifact, _) = agent else {
            return XCTFail("Agent observation was rejected: \(agent)")
        }
        XCTAssertEqual(agentArtifact.chapters[1].endMilliseconds, 30_000)
        XCTAssertEqual(agentArtifact.chapters[1].sourceEpisodeId, EpisodeId(high: 99, low: 7))

        let rejected = qualifyPublisherChapterObservation(observation: PublisherChapterObservation(
            episodeId: episodeID,
            podcastId: podcastID,
            resolvedSourceUrl: "https://example.test/chapters.json",
            contentType: "text/html",
            payloadDigest: Self.digest(publisherData),
            payload: publisherData,
            generatedAt: observedAt,
            durationMilliseconds: 100_000
        ))
        XCTAssertEqual(rejected, .rejected(reason: .invalidContentType))
    }

    private static func digest(_ data: Data) -> ContentDigest {
        let bytes = Array(SHA256.hash(data: data))
        func word(_ offset: Int) -> UInt64 {
            bytes[offset..<(offset + 8)].reduce(0) { ($0 << 8) | UInt64($1) }
        }
        return ContentDigest(word0: word(0), word1: word(8), word2: word(16), word3: word(24))
    }
}
