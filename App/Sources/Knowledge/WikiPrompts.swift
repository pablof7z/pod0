import Foundation

// MARK: - Wiki prompts

/// Centralised prompt templates for the wiki compiler. Pure functions —
/// no side effects, no I/O. Splitting them out keeps `WikiGenerator`
/// focused on orchestration and makes the prompts diff-reviewable.
///
/// All prompts share a hard rule, restated in the system message: every
/// claim returned must include at least one citation pointing at a real
/// `(episode_id, start_ms, end_ms)` span, and `quoteSnippet` must be a
/// verbatim ≤125-char excerpt the verifier can substring-match against
/// the original transcript chunk.
enum WikiPrompts {

    /// Maximum length of a verbatim snippet supplied back by the LLM.
    /// Mirrors `WikiCitation.maxSnippetLength` for symmetry.
    static let maxSnippetLength = WikiCitation.maxSnippetLength

    // MARK: - System prompt

    /// The contract every wiki-compile turn opens with. Locks the model
    /// into a JSON schema and the citation-or-it-doesn't-render rule.
    static let system: String = """
    You are the editor of an editorial-grade LLM wiki for a podcast \
    listener's personal library. Your job is to synthesize a calm, \
    Britannica-style article that is *fully grounded in the transcript \
    spans the user has provided*.

    Hard rules — violation invalidates the response:
    1. Every claim you write must cite at least one transcript chunk by \
       (episode_id, start_ms, end_ms).
    2. quote_snippet must be a verbatim excerpt of at most \
       \(maxSnippetLength) characters from the cited chunk.
    3. Do not invent episodes, speakers, or timestamps. Do not paraphrase \
       a chunk you were not given.
    4. If a section has no supporting evidence, return it empty rather \
       than filling it.
    5. Use plain ASCII punctuation and Markdown-free body text. The \
       client wraps the response in its own typography.

    Respond as a single JSON object with the schema:
    {
      "title": String,
      "summary": String (≤320 chars),
      "confidence": Number (0…1),
      "sections": [
        {
          "heading": String,
          "kind": "definition"|"whoDiscusses"|"evolution"|"consensus" \
            |"contradictions"|"related"|"citations"|"freeform",
          "claims": [
            {
              "text": String,
              "confidence": "high"|"medium"|"low",
              "citations": [
                { "episode_id": UUID, "start_ms": Int, "end_ms": Int,
                  "quote_snippet": String, "speaker": String? }
              ]
            }
          ]
        }
      ]
    }
    """

    // MARK: - Public templates

    /// Topic page — the dominant kind. Renders sections per UX-04.
    static func topic(
        topic: String,
        scope: WikiScope,
        chunks: [RAGChunk]
    ) -> String {
        let scopeLine = describe(scope: scope)
        let sources = renderChunks(chunks)
        return """
        Compile a TOPIC page about: "\(topic)".
        Scope: \(scopeLine).

        Use these transcript spans as your only evidence:
        \(sources)

        Required sections (omit a section if no evidence supports it):
          • Definition (1 paragraph, evidence-graded)
          • Who's discussed it (speaker list with episode counts)
          • Consensus
          • Contradictions
          • Related topics
          • Citations (one per cited episode)
        """
    }

    /// Person page — thin variant deferring depth to UX-13 speaker
    /// profiles. The wiki page lists role + top-3 claims with citations.
    static func person(
        name: String,
        scope: WikiScope,
        chunks: [RAGChunk]
    ) -> String {
        let sources = renderChunks(chunks)
        return """
        Compile a PERSON page about: "\(name)".
        Scope: \(describe(scope: scope)).

        Use these transcript spans as your only evidence:
        \(sources)

        Required sections:
          • Definition (1 paragraph: role, why they appear in this library)
          • Top 3 claims with citations
          • Episodes they appear on
        Keep the page short (≤4 sections, ≤8 claims). The full bio lives \
        in the Speaker Profile surface.
        """
    }

    /// Show summary page — one podcast, end-to-end.
    static func show(
        showName: String,
        scope: WikiScope,
        chunks: [RAGChunk]
    ) -> String {
        let sources = renderChunks(chunks)
        return """
        Compile a SHOW summary page for: "\(showName)".
        Scope: \(describe(scope: scope)).

        Use these transcript spans as your only evidence:
        \(sources)

        Required sections:
          • Definition (1 paragraph: format, host, dominant themes)
          • Recurring topics (cluster the most-cited concepts)
          • Notable episodes (3–5 with one-line takeaways)
        Treat this as a magazine cover blurb — calm, factual, no hype.
        """
    }

    /// Audit pass — given a *prior* page and a fresh chunk set, ask the
    /// model what changed since last regen. Drives the freshness UI.
    static func audit(
        prior: WikiPage,
        chunks: [RAGChunk]
    ) -> String {
        let sources = renderChunks(chunks)
        return """
        Audit the following wiki page against the supplied evidence.

        Page title: \(prior.title)
        Page kind: \(prior.kind.rawValue)
        Existing claims: \(prior.allClaims.count)

        New evidence chunks:
        \(sources)

        Return the same JSON schema as a fresh compile — the client will \
        atomically swap. For any prior claim still supported, retain its \
        confidence band; for claims newly contradicted, drop confidence \
        to "low" and add a contradicting claim alongside.
        """
    }

    // MARK: - Helpers

    /// Renders a chunk list as numbered evidence the LLM can index into.
    private static func renderChunks(_ chunks: [RAGChunk]) -> String {
        guard !chunks.isEmpty else {
            return "  (no evidence available — return empty sections)"
        }
        return chunks.enumerated().map { idx, chunk in
            let speaker = chunk.speaker ?? "unknown speaker"
            return """
              [\(idx + 1)] episode_id=\(chunk.episodeID.uuidString)
                  span=[\(chunk.startMS),\(chunk.endMS)) speaker=\(speaker)
                  text: \(chunk.text)
            """
        }.joined(separator: "\n")
    }

    private static func describe(scope: WikiScope) -> String {
        switch scope {
        case .global: "global library"
        case .podcast(let id): "single podcast (id=\(id.uuidString))"
        }
    }
}
