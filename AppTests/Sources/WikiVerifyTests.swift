import XCTest
@testable import Podcastr

@MainActor
final class WikiVerifyTests: XCTestCase {

    // MARK: - Fixtures

    private let episodeID = UUID()
    private let podcastID = UUID()

    private func makeChunk(
        text: String,
        startMS: Int = 0,
        endMS: Int = 30_000
    ) -> RAGChunk {
        RAGChunk(
            episodeID: episodeID,
            podcastID: podcastID,
            startMS: startMS,
            endMS: endMS,
            text: text,
            score: 1.0
        )
    }

    private func makeCitation(
        startMS: Int = 0,
        endMS: Int = 30_000,
        snippet: String,
        episodeID: UUID? = nil
    ) -> WikiCitation {
        WikiCitation(
            episodeID: episodeID ?? self.episodeID,
            startMS: startMS,
            endMS: endMS,
            quoteSnippet: snippet
        )
    }

    private func makePage(claims: [WikiClaim]) -> WikiPage {
        let section = WikiSection(
            heading: "Definition",
            kind: .definition,
            ordinal: 0,
            claims: claims
        )
        return WikiPage(
            slug: "ozempic",
            title: "Ozempic",
            kind: .topic,
            scope: .global,
            summary: "Test page",
            sections: [section],
            confidence: 0.8
        )
    }

    // MARK: - Citation snippet clamp

    func testCitationClampsLongSnippet() {
        let snippet = String(repeating: "x", count: 200)
        let citation = WikiCitation(
            episodeID: episodeID,
            startMS: 0,
            endMS: 1_000,
            quoteSnippet: snippet
        )
        XCTAssertLessThanOrEqual(citation.quoteSnippet.count, WikiCitation.maxSnippetLength)
        XCTAssertTrue(citation.quoteSnippet.hasSuffix("…"))
    }

    func testCitationKeepsShortSnippet() {
        let citation = WikiCitation(
            episodeID: episodeID,
            startMS: 0,
            endMS: 1_000,
            quoteSnippet: "uncoupling effect on mitochondria"
        )
        XCTAssertEqual(citation.quoteSnippet, "uncoupling effect on mitochondria")
    }

    // MARK: - Slug normalization

    func testSlugNormalization() {
        XCTAssertEqual(WikiPage.normalize(slug: "Mitochondrial Uncoupling!"), "mitochondrial-uncoupling")
        XCTAssertEqual(WikiPage.normalize(slug: "  GLP-1 / Ozempic "), "glp-1-ozempic")
        XCTAssertEqual(WikiPage.normalize(slug: "—"), "untitled")
    }

    // MARK: - Verifier — claims with verifiable citations are kept

    func testVerifierKeepsClaimsWithExactSnippetMatch() async throws {
        let chunkText = "the uncoupling effect on mitochondria can be measured"
        let rag = InMemoryRAGSearch(chunks: [makeChunk(text: chunkText)])
        let citation = makeCitation(snippet: "uncoupling effect on mitochondria")
        let claim = WikiClaim(
            text: "Ozempic affects mitochondria.",
            citations: [citation],
            confidence: .medium
        )
        let page = makePage(claims: [claim])

        let result = try await WikiVerifier(rag: rag).verify(page)

        XCTAssertEqual(result.keptClaims, 1)
        XCTAssertEqual(result.droppedClaims, 0)
        XCTAssertEqual(
            result.page.sections.first?.claims.first?.citations.first?.verificationConfidence,
            .high
        )
    }

    // MARK: - Verifier — fabricated citations cause claim drop

    func testVerifierDropsClaimWithUnresolvedCitation() async throws {
        // RAG store contains nothing — no chunk lookup will succeed.
        let rag = InMemoryRAGSearch(chunks: [])
        let claim = WikiClaim(
            text: "Ozempic cures everything.",
            citations: [makeCitation(snippet: "this is not in any chunk")],
            confidence: .high
        )
        let page = makePage(claims: [claim])

        let result = try await WikiVerifier(rag: rag).verify(page)

        XCTAssertEqual(result.keptClaims, 0)
        XCTAssertEqual(result.droppedClaims, 1)
        XCTAssertTrue(result.page.sections.first?.claims.isEmpty == true)
    }

    // MARK: - Verifier — unsourced claims are dropped unless general knowledge

    func testVerifierDropsUnsourcedClaim() async throws {
        let rag = InMemoryRAGSearch(chunks: [makeChunk(text: "anything")])
        let claim = WikiClaim(text: "Sourceless claim.", citations: [])
        let page = makePage(claims: [claim])

        let result = try await WikiVerifier(rag: rag).verify(page)
        XCTAssertEqual(result.droppedClaims, 1)
    }

    func testVerifierKeepsGeneralKnowledgeClaim() async throws {
        let rag = InMemoryRAGSearch(chunks: [makeChunk(text: "anything")])
        let claim = WikiClaim(
            text: "Water boils at 100C.",
            citations: [],
            confidence: .high,
            isGeneralKnowledge: true
        )
        let page = makePage(claims: [claim])

        let result = try await WikiVerifier(rag: rag).verify(page)
        XCTAssertEqual(result.keptClaims, 1)
        // General-knowledge survival demotes confidence to low.
        XCTAssertEqual(result.page.sections.first?.claims.first?.confidence, .low)
    }

    /// General-knowledge passes ONLY inside a Definition section. The LLM
    /// would otherwise sprinkle `general_knowledge: true` across Evolution
    /// / Consensus / Contradictions to launder claims it couldn't source;
    /// outside Definition the verifier still drops them.
    func testVerifierDropsGeneralKnowledgeClaimOutsideDefinition() async throws {
        let rag = InMemoryRAGSearch(chunks: [makeChunk(text: "anything")])
        let claim = WikiClaim(
            text: "Some unsourced consensus assertion.",
            citations: [],
            confidence: .high,
            isGeneralKnowledge: true
        )
        let section = WikiSection(
            heading: "Consensus",
            kind: .consensus,
            ordinal: 1,
            claims: [claim]
        )
        let page = WikiPage(
            slug: "ozempic",
            title: "Ozempic",
            kind: .topic,
            scope: .global,
            summary: "Test page",
            sections: [section]
        )

        let result = try await WikiVerifier(rag: rag).verify(page)
        XCTAssertEqual(result.keptClaims, 0)
        XCTAssertEqual(result.droppedClaims, 1)
    }

    // MARK: - Verifier — partial unresolved citations demote confidence

    func testVerifierDemotesClaimWithPartiallyMissingCitations() async throws {
        let chunkText = "tim ferriss interviewed huberman about uncoupling"
        let rag = InMemoryRAGSearch(chunks: [makeChunk(text: chunkText)])
        let goodCitation = makeCitation(snippet: "uncoupling")
        let badCitation = makeCitation(
            startMS: 60_000,
            endMS: 90_000,
            snippet: "this won't resolve",
            episodeID: UUID()
        )
        let claim = WikiClaim(
            text: "Mixed claim.",
            citations: [goodCitation, badCitation],
            confidence: .high
        )
        let page = makePage(claims: [claim])

        let result = try await WikiVerifier(rag: rag).verify(page)
        XCTAssertEqual(result.keptClaims, 1)
        XCTAssertEqual(result.droppedCitations, 1)
        // High demoted by one band because one of two citations failed.
        XCTAssertNotEqual(result.page.sections.first?.claims.first?.confidence, .high)
    }

    // MARK: - Verifier — survival rate moves the page-level confidence

    func testVerifierBlendsPageConfidenceWithSurvivalRate() async throws {
        let chunkText = "phrase one and phrase two"
        let rag = InMemoryRAGSearch(chunks: [makeChunk(text: chunkText)])
        let kept = WikiClaim(
            text: "Kept",
            citations: [makeCitation(snippet: "phrase one")],
            confidence: .high
        )
        let dropped = WikiClaim(
            text: "Dropped",
            citations: [makeCitation(snippet: "this won't match", episodeID: UUID())]
        )
        let page = makePage(claims: [kept, dropped])

        let result = try await WikiVerifier(rag: rag).verify(page)
        // 1/2 survival × 0.7 + 0.8 × 0.3 = 0.59
        XCTAssertEqual(result.page.confidence, 0.59, accuracy: 0.01)
    }

    // MARK: - Fuzzy match

    func testFuzzyMatchHitsOnTokenOverlap() {
        let chunk = "the uncoupling effect on mitochondria has been studied"
        let snippet = "uncoupling effect mitochondria"
        XCTAssertTrue(WikiCitation.fuzzyMatch(snippet: snippet, in: chunk))
    }

    func testFuzzyMatchMissesOnPoorOverlap() {
        let chunk = "completely different topic about cars"
        let snippet = "uncoupling effect mitochondria"
        XCTAssertFalse(WikiCitation.fuzzyMatch(snippet: snippet, in: chunk))
    }

    // MARK: - Storage round-trip

    func testStorageRoundTrip() throws {
        let tmp = FileManager.default.temporaryDirectory
            .appendingPathComponent("wiki-test-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: tmp) }

        let storage = WikiStorage(root: tmp)
        let page = makePage(claims: [
            WikiClaim(
                text: "Test",
                citations: [makeCitation(snippet: "snippet")],
                confidence: .medium
            )
        ])
        try storage.write(page)

        let loaded = try storage.read(slug: page.slug, scope: page.scope)
        XCTAssertNotNil(loaded)
        XCTAssertEqual(loaded?.title, page.title)
        XCTAssertEqual(loaded?.sections.first?.claims.first?.text, "Test")

        let inventory = try storage.loadInventory()
        XCTAssertEqual(inventory.entries.count, 1)
        XCTAssertEqual(inventory.entries.first?.slug, page.slug)
    }

    func testStorageDeleteRemovesEntry() throws {
        let tmp = FileManager.default.temporaryDirectory
            .appendingPathComponent("wiki-test-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: tmp) }

        let storage = WikiStorage(root: tmp)
        let page = makePage(claims: [])
        try storage.write(page)
        try storage.delete(slug: page.slug, scope: page.scope)

        let inventory = try storage.loadInventory()
        XCTAssertEqual(inventory.entries.count, 0)
        XCTAssertNil(try storage.read(slug: page.slug, scope: page.scope))
    }

    // MARK: - Triggers

    func testTriggersProduceJobsForExistingPagesOnly() {
        let inventory = WikiInventory(
            version: 1,
            entries: [
                .init(
                    slug: "ozempic",
                    title: "Ozempic",
                    kind: .topic,
                    scope: .global,
                    summary: "",
                    confidence: 0.7,
                    generatedAt: Date(),
                    model: "test",
                    citationCount: 3,
                    fileURL: URL(fileURLWithPath: "/tmp/x.json")
                )
            ],
            updatedAt: Date()
        )

        let event = WikiTriggers.Event.episodeTranscribed(
            episodeID: UUID(),
            podcastID: UUID(),
            extractedTopics: ["Ozempic", "Mitochondria"],
            extractedPeople: []
        )

        let jobs = WikiTriggers.jobs(for: event, inventory: inventory)
        XCTAssertEqual(jobs.count, 1)
        XCTAssertEqual(jobs.first?.slug, "ozempic")
        XCTAssertEqual(jobs.first?.reason, .newEvidence)
    }

    func testTriggersFlagsAllPagesOnModelMigration() {
        let inventory = WikiInventory(
            version: 1,
            entries: [
                .init(
                    slug: "a",
                    title: "A",
                    kind: .topic,
                    scope: .global,
                    summary: "",
                    confidence: 0.5,
                    generatedAt: Date(),
                    model: "test",
                    citationCount: 0,
                    fileURL: URL(fileURLWithPath: "/tmp/a.json")
                ),
                .init(
                    slug: "b",
                    title: "B",
                    kind: .topic,
                    scope: .global,
                    summary: "",
                    confidence: 0.5,
                    generatedAt: Date(),
                    model: "test",
                    citationCount: 0,
                    fileURL: URL(fileURLWithPath: "/tmp/b.json")
                )
            ],
            updatedAt: Date()
        )

        let jobs = WikiTriggers.jobs(
            for: .modelMigrated(newModel: "openai/gpt-5"),
            inventory: inventory
        )
        XCTAssertEqual(jobs.count, 2)
        XCTAssertTrue(jobs.allSatisfy { $0.reason == .modelMigration })
    }

    // MARK: - End-to-end generator with stubbed LLM

    func testGeneratorEndToEndWithStubbedLLM() async throws {
        let chunkText = "ozempic is a glp-1 agonist with strong weight-loss effects"
        let chunk = makeChunk(text: chunkText)
        let rag = InMemoryRAGSearch(chunks: [chunk])

        let stubJSON = """
        {
          "title": "Ozempic",
          "summary": "Synthetic GLP-1 agonist used for weight loss.",
          "confidence": 0.78,
          "sections": [
            {
              "heading": "Definition",
              "kind": "definition",
              "claims": [
                {
                  "text": "Ozempic is a GLP-1 agonist.",
                  "confidence": "high",
                  "citations": [
                    {
                      "episode_id": "\(episodeID.uuidString)",
                      "start_ms": 0,
                      "end_ms": 30000,
                      "quote_snippet": "ozempic is a glp-1 agonist"
                    }
                  ]
                }
              ]
            }
          ]
        }
        """

        let tmp = FileManager.default.temporaryDirectory
            .appendingPathComponent("wiki-gen-test-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: tmp) }

        let generator = WikiGenerator(
            rag: rag,
            client: .stubbed(json: stubJSON),
            storage: WikiStorage(root: tmp),
            model: "test/stub"
        )

        let result = try await generator.compileTopic(topic: "Ozempic", scope: .global)
        XCTAssertEqual(result.keptClaims, 1)
        XCTAssertEqual(result.page.title, "Ozempic")
        XCTAssertEqual(result.page.sections.first?.claims.first?.confidence, .high)
        try generator.persist(result.page)

        let loaded = try generator.storage.read(slug: result.page.slug, scope: .global)
        XCTAssertEqual(loaded?.title, "Ozempic")
    }
}
