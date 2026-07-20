import Pod0Core
import XCTest

final class ChapterModelPlanBindingTests: XCTestCase {
    private enum PlanError: Error { case notReady }

    func testSwiftBindingReadsGeneratedAndEnrichedGoldenPlans() throws {
        let generated = try ready(planChapterModelRequest(input: input(artifact: nil)))
        XCTAssertEqual(generated.provider, "openrouter")
        XCTAssertEqual(generated.model, "openai/gpt-4o-mini")
        XCTAssertEqual(generated.responseFormat, .jsonObject)
        XCTAssertEqual(generated.maximumCompletionBytes, 1_048_576)
        XCTAssertEqual(generated.expectedArtifactSource, .generated)
        XCTAssertEqual(
            generated.systemPrompt,
            try fixture("chapter-model-generation-system-v1")
        )
        XCTAssertEqual(
            generated.userPrompt,
            try fixture("chapter-model-generation-user-v1")
        )

        let enriched = try ready(planChapterModelRequest(input: input(
            artifact: publisherArtifact()
        )))
        XCTAssertEqual(enriched.expectedArtifactSource, .publisherEnriched)
        guard case .enrich(let artifact) = enriched.mode else {
            return XCTFail("Publisher input did not select enrichment")
        }
        XCTAssertEqual(artifact.chapters.map(\.title), ["Opening", "Deeper work"])
        XCTAssertEqual(
            enriched.systemPrompt,
            try fixture("chapter-model-enrichment-system-v1")
        )
        XCTAssertEqual(
            enriched.userPrompt,
            try fixture("chapter-model-enrichment-user-v1")
        )
    }

    func testSwiftBindingExposesDeterministicVersionAndTypedStates() throws {
        let desired = planChapterModelDesiredState(input: ChapterModelDesiredStateInput(
            transcriptContentDigest: digest,
            configuredModel: "openai/gpt-4o-mini",
            selectedChapterSource: .generated
        ))
        guard case .compile(let inputVersion) = desired else {
            return XCTFail("Generated chapters should remain derivable")
        }
        XCTAssertEqual(
            inputVersion,
            "a474dab4fdd3631c3709a627114dc1978b0cdb77fef47b60044842523e721f5f"
        )
        XCTAssertEqual(
            planChapterModelDesiredState(input: ChapterModelDesiredStateInput(
                transcriptContentDigest: digest,
                configuredModel: "openai/gpt-4o-mini",
                selectedChapterSource: .agentComposed
            )),
            .preserveAgentComposed
        )

        var invalid = input(artifact: nil, configuredModel: "   ")
        XCTAssertEqual(planChapterModelRequest(input: invalid), .invalidConfiguration)
        invalid = input(artifact: nil)
        invalid = ChapterModelPlanInput(
            episode: invalid.episode,
            requestedTranscriptVersionId: invalid.requestedTranscriptVersionId,
            requestedTranscriptContentDigest: invalid.requestedTranscriptContentDigest,
            selectedTranscript: nil,
            selectedChapterArtifact: nil,
            expectedChapterSelectionRevision: invalid.expectedChapterSelectionRevision,
            configuredModel: invalid.configuredModel
        )
        XCTAssertEqual(planChapterModelRequest(input: invalid), .transcriptUnavailable)
    }

    private var digest: ContentDigest {
        ContentDigest(
            word0: 0x0505_0505_0505_0505,
            word1: 0x0505_0505_0505_0505,
            word2: 0x0505_0505_0505_0505,
            word3: 0x0505_0505_0505_0505
        )
    }

    private func input(
        artifact: ChapterArtifactInput?,
        configuredModel: String = "openai/gpt-4o-mini"
    ) -> ChapterModelPlanInput {
        ChapterModelPlanInput(
            episode: ChapterModelEpisodeInput(
                episodeId: EpisodeId(high: 1, low: 2),
                podcastId: PodcastId(high: 3, low: 4),
                title: "A Calm Test",
                description: "Ignored by prompt v1",
                durationSeconds: 125.75
            ),
            requestedTranscriptVersionId: TranscriptVersionId(high: 5, low: 6),
            requestedTranscriptContentDigest: digest,
            selectedTranscript: ChapterModelTranscriptInput(
                transcriptVersionId: TranscriptVersionId(high: 5, low: 6),
                transcriptContentDigest: digest,
                segments: [
                    .init(startSeconds: 0.4, text: " Opening thought "),
                    .init(startSeconds: 12.5, text: "Second idea"),
                ]
            ),
            selectedChapterArtifact: artifact,
            expectedChapterSelectionRevision: StateRevision(value: 0),
            configuredModel: configuredModel
        )
    }

    private func publisherArtifact() -> ChapterArtifactInput {
        ChapterArtifactInput(
            episodeId: EpisodeId(high: 1, low: 2),
            podcastId: PodcastId(high: 3, low: 4),
            sourceRevision: "publisher-v1",
            provenance: ChapterArtifactProvenance(
                source: .publisher,
                provider: nil,
                model: nil,
                policyVersion: 0,
                sourcePayloadDigest: ContentDigest(word0: 7, word1: 7, word2: 7, word3: 7),
                transcriptVersionId: nil,
                transcriptContentDigest: nil,
                legacyImport: nil
            ),
            generatedAt: UnixTimestampMilliseconds(value: 1_700_000_000_000),
            durationMilliseconds: 125_750,
            chapters: [chapter(0, "Opening"), chapter(60_000, "Deeper work")],
            adSpanEvaluation: .notEvaluated,
            adSpans: []
        )
    }

    private func chapter(_ start: UInt64, _ title: String) -> ChapterInput {
        ChapterInput(
            startMilliseconds: start,
            endMilliseconds: nil,
            title: title,
            summary: nil,
            imageUrl: nil,
            linkUrl: nil,
            includeInTableOfContents: true,
            sourceEpisodeId: nil
        )
    }

    private func ready(_ plan: ChapterModelPlan) throws -> PlannedChapterModelRequest {
        guard case .ready(let request) = plan else {
            XCTFail("Expected a ready model plan, got \(plan)")
            throw PlanError.notReady
        }
        return request
    }

    private func fixture(_ name: String) throws -> String {
        let url = try XCTUnwrap(Bundle(for: Self.self).url(
            forResource: name,
            withExtension: "txt"
        ))
        return try String(contentsOf: url, encoding: .utf8)
            .trimmingCharacters(in: .newlines)
    }
}
