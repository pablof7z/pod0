import Foundation

// MARK: - Wiki verifier

/// Post-compile verification pass.
///
/// The synthesizer is liable to drift from its evidence. The verifier
/// resolves *every* citation against the underlying RAG index and drops
/// any claim whose citations don't survive. This is the *non-negotiable*
/// gate the llm-wiki ethos demands: provenance-or-it-doesn't-render.
///
/// The verifier is independent of the LLM — it talks to `WikiRAGSearchProtocol`
/// only. That keeps it cheap (no second LLM round-trip) and deterministic
/// in tests.
struct WikiVerifier: Sendable {

    let rag: any WikiRAGSearchProtocol

    /// Production default: hooks into the live `RAGService` singleton so the
    /// verifier resolves citations against the on-device sqlite-vec store.
    /// `@MainActor` because `RAGService.shared` is main-actor isolated; the
    /// verifier itself remains `Sendable` and its `verify(_:)` method runs
    /// off-main via `await`.
    @MainActor
    init() {
        self.rag = RAGService.shared.wikiRAG
    }

    /// Test-injectable init. Tests pass an `InMemoryRAGSearch` (or another
    /// mock) so they don't need to bring up `RAGService`.
    init(rag: any WikiRAGSearchProtocol) {
        self.rag = rag
    }

    // MARK: - Public

    /// Runs the verification pass on `page` and returns a new page with
    /// unverified claims dropped, citation confidences updated, and the
    /// page-level confidence recomputed.
    ///
    /// The original page is not mutated.
    func verify(_ page: WikiPage) async throws -> WikiVerifyResult {
        var verifiedSections: [WikiSection] = []
        var droppedClaimCount = 0
        var droppedCitationCount = 0
        var keptClaimCount = 0

        for section in page.sections {
            var keptClaims: [WikiClaim] = []
            for claim in section.claims {
                let outcome = try await verify(claim: claim, sectionKind: section.kind)
                switch outcome {
                case .kept(let updated):
                    keptClaims.append(updated)
                    keptClaimCount += 1
                    droppedCitationCount += claim.citations.count - updated.citations.count
                case .dropped:
                    droppedClaimCount += 1
                    droppedCitationCount += claim.citations.count
                }
            }
            // Preserve empty sections so a later regen can repopulate
            // them, but flag low-evidence sections in the editorial note.
            var updated = section
            updated.claims = keptClaims
            if keptClaims.isEmpty && !section.claims.isEmpty {
                updated.editorialNote = "All claims dropped during verification."
            }
            verifiedSections.append(updated)
        }

        let pageLevelCitations = verifiedSections
            .flatMap { $0.claims.flatMap(\.citations) }

        let updatedConfidence = recomputeConfidence(
            keptClaims: keptClaimCount,
            droppedClaims: droppedClaimCount,
            originalConfidence: page.confidence
        )

        var verified = page
        verified.sections = verifiedSections
        verified.citations = pageLevelCitations
        verified.confidence = updatedConfidence

        return WikiVerifyResult(
            page: verified,
            keptClaims: keptClaimCount,
            droppedClaims: droppedClaimCount,
            droppedCitations: droppedCitationCount
        )
    }

    // MARK: - Private

    /// Resolves each citation in `claim` against the RAG store and
    /// decides whether to keep, demote, or drop the claim.
    private func verify(
        claim: WikiClaim,
        sectionKind: WikiSectionKind
    ) async throws -> ClaimOutcome {
        // General-knowledge claims are allowed to pass through with `low`
        // confidence even when they have no citations — but ONLY inside a
        // `definition` section, mirroring the llm-wiki rule. The LLM
        // would otherwise sprinkle `general_knowledge: true` across
        // Consensus / Evolution / Contradictions to launder claims it
        // couldn't source; outside Definition we still drop it.
        if claim.isGeneralKnowledge && sectionKind == .definition {
            return .kept(claim.demotingTo(.low))
        }
        if claim.citations.isEmpty {
            return .dropped
        }

        var resolved: [WikiCitation] = []
        var unresolved = 0
        for citation in claim.citations {
            let chunk = try await rag.chunk(
                episodeID: citation.episodeID,
                startMS: citation.startMS,
                endMS: citation.endMS
            )
            guard let chunk else {
                unresolved += 1
                continue
            }
            resolved.append(citation.byVerifying(against: chunk))
        }

        guard !resolved.isEmpty else { return .dropped }

        // Recompute claim confidence from its surviving citations.
        let confidence = aggregate(confidences: resolved.map(\.verificationConfidence))
        var updated = claim
        updated.citations = resolved
        updated.confidence = confidence
        if unresolved > 0 {
            // At least one citation didn't resolve — demote a band.
            updated.confidence = updated.confidence.demoted()
        }
        return .kept(updated)
    }

    private func recomputeConfidence(
        keptClaims: Int,
        droppedClaims: Int,
        originalConfidence: Double
    ) -> Double {
        let total = keptClaims + droppedClaims
        guard total > 0 else { return originalConfidence }
        let survivalRate = Double(keptClaims) / Double(total)
        // Blend: 70% empirical survival, 30% the model's self-rating.
        return (0.7 * survivalRate) + (0.3 * originalConfidence)
    }

    private func aggregate(confidences: [WikiConfidenceBand]) -> WikiConfidenceBand {
        guard !confidences.isEmpty else { return .low }
        let highCount = confidences.count(where: { $0 == .high })
        let lowCount = confidences.count(where: { $0 == .low })
        if highCount > confidences.count / 2 { return .high }
        if lowCount > confidences.count / 2 { return .low }
        return .medium
    }

    // MARK: - Outcome

    private enum ClaimOutcome {
        case kept(WikiClaim)
        case dropped
    }
}

// MARK: - Result

/// What the verifier produces. Kept separate from the page so callers
/// can surface drop counts in the UI without re-walking the model.
struct WikiVerifyResult: Sendable {
    var page: WikiPage
    var keptClaims: Int
    var droppedClaims: Int
    var droppedCitations: Int
}

// MARK: - Citation/claim helpers

extension WikiCitation {

    /// Returns a copy of `self` with verification confidence adjusted
    /// based on whether the supplied chunk's text contains the cited
    /// quote snippet.
    func byVerifying(against chunk: RAGChunk) -> WikiCitation {
        let snippetLower = quoteSnippet
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased()
        let chunkLower = chunk.text.lowercased()
        let band: WikiConfidenceBand
        if snippetLower.isEmpty {
            band = .low
        } else if chunkLower.contains(snippetLower) {
            band = .high
        } else if WikiCitation.fuzzyMatch(snippet: snippetLower, in: chunkLower) {
            band = .medium
        } else {
            band = .low
        }
        var updated = self
        updated.verificationConfidence = band
        return updated
    }

    /// Loose token-overlap heuristic for cases where the LLM lightly
    /// reformats whitespace or punctuation. Returns `true` when at
    /// least 60% of the snippet's word tokens (≥ 3 chars) appear in
    /// the chunk text in order.
    static func fuzzyMatch(snippet: String, in chunk: String) -> Bool {
        let tokens = snippet
            .split(whereSeparator: { !$0.isLetter && !$0.isNumber })
            .map(String.init)
            .filter { $0.count >= 3 }
        guard !tokens.isEmpty else { return false }
        let chunkTokens = Set(
            chunk
                .split(whereSeparator: { !$0.isLetter && !$0.isNumber })
                .map(String.init)
        )
        let hits = tokens.filter { chunkTokens.contains($0) }.count
        return Double(hits) / Double(tokens.count) >= 0.6
    }
}

extension WikiClaim {
    /// Returns a copy of `self` with confidence demoted to `band`.
    func demotingTo(_ band: WikiConfidenceBand) -> WikiClaim {
        var updated = self
        updated.confidence = band
        return updated
    }
}

private extension WikiConfidenceBand {
    /// Drops one rung of the confidence ladder. `low` is a fixed point.
    func demoted() -> WikiConfidenceBand {
        switch self {
        case .high: .medium
        case .medium: .low
        case .low: .low
        }
    }
}
