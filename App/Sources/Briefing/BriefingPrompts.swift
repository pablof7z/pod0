import Foundation

// MARK: - BriefingPrompts

/// Script templates that frame the LLM call inside `BriefingComposer`.
///
/// These map 1:1 to `BriefingStyle` cases and to the four preset tiles in the
/// Compose surface (UX-08 §3 — "Quick presets"). Each template emits:
///   1. A *system prompt* establishing tone, structure, and citation rules.
///   2. A *user prompt* parameterised by `BriefingRequest`, the gathered RAG
///      candidates, and any optional freeform query.
///
/// The composer hands these strings to OpenRouter unmodified; the model is
/// expected to return JSON describing segments. The system prompt enforces the
/// JSON schema so we never have to parse markdown.
enum BriefingPrompts {

    // MARK: Public entry points

    static func systemPrompt(for style: BriefingStyle) -> String {
        let base = SharedSystemPrompt.base
        let voice = voiceFraming(for: style)
        return [base, voice, SharedSystemPrompt.schemaContract].joined(separator: "\n\n")
    }

    static func userPrompt(
        for request: BriefingRequest,
        candidates: [RAGCandidate],
        wikiTitles: [String]
    ) -> String {
        var parts: [String] = []
        parts.append(headerLine(for: request))
        parts.append(scopeLine(for: request))
        if let q = request.freeformQuery, !q.isEmpty {
            parts.append("FREEFORM REQUEST: \(q)")
        }
        if !wikiTitles.isEmpty {
            parts.append("RELATED WIKI PAGES: \(wikiTitles.joined(separator: ", "))")
        }
        parts.append("CANDIDATE SOURCES:")
        for (index, c) in candidates.enumerated() {
            parts.append("[\(index + 1)] \(c.sourceLabel) — \(c.text)")
        }
        parts.append("Compose the briefing now. Reply with JSON only.")
        return parts.joined(separator: "\n\n")
    }

    // MARK: - Style-specific framing

    private static func voiceFraming(for style: BriefingStyle) -> String {
        switch style {
        case .morning:
            return """
            STYLE: Morning briefing — calm, NPR-cadence. Open with the day, not \
            the news. Six segments max. End with a 'today's threads' segment.
            """
        case .weeklyTLDR:
            return """
            STYLE: Weekly TLDR — recap the week's most consequential threads. \
            Lead with the single biggest story; finish with what to listen to next.
            """
        case .catchUpOnShow:
            return """
            STYLE: Catch-up — the user has been off this show. Reconstruct the \
            arc across recent episodes; treat each as a chapter, not a list item.
            """
        case .topicAcrossLibrary:
            return """
            STYLE: Topic deep-dive — synthesise across shows. Where hosts disagree, \
            name the disagreement. Where they agree, say so plainly. Resolve, don't average.
            """
        }
    }

    private static func headerLine(for request: BriefingRequest) -> String {
        let formatter = DateFormatter()
        formatter.dateStyle = .full
        let dateLabel = formatter.string(from: request.requestedAt)
        return "DATE: \(dateLabel) · TARGET LENGTH: \(request.length.displayLabel)"
    }

    private static func scopeLine(for request: BriefingRequest) -> String {
        switch request.scope {
        case .mySubscriptions: "SCOPE: All the user's subscribed shows."
        case .thisShow:        "SCOPE: One show only — focus tightly."
        case .thisTopic:       "SCOPE: One topic across the library."
        case .thisWeek:        "SCOPE: Past seven days of episodes."
        }
    }
}

// MARK: - Shared system prompt fragments

private enum SharedSystemPrompt {
    static let base = """
    You are the editor of a personal podcast briefing. Your audience is one \
    listener; you have been listening on their behalf. Your output is read aloud \
    by a TTS narrator and paired with original-audio quotes the user can branch \
    into. Your job is to compress, attribute, and decide what matters.

    PRINCIPLES
    - Lead with the only thing that matters today; cut what merely happened.
    - Every factual sentence carries a source. Sentences without one are framed \
      as summary, not claim.
    - Hosts disagree more than they agree. When they do, name the disagreement.
    - Use original-audio quotes sparingly — only when the host's voice carries \
      something the paraphrase would lose.
    - Length is a contract. If the request is 8 minutes, write 8 minutes.
    """

    static let schemaContract = """
    OUTPUT CONTRACT — JSON only, no prose:
    {
      "title": String,                         // editorial title, ~3 words
      "subtitle": String,                      // "<minutes> min · drawn from <n> episodes"
      "segments": [
        {
          "title": String,                     // segment headline
          "body_text": String,                 // narration body
          "target_seconds": Number,            // pacing estimate
          "attributions": [                    // sources cited in this segment
            { "candidate_index": Number, "label": String }
          ],
          "quotes": [                          // optional original-audio splices
            {
              "candidate_index": Number,
              "insert_after_char": Number,     // offset into body_text
              "transcript_text": String
            }
          ]
        }
      ]
    }

    Reply with the JSON object only — no markdown fences, no commentary.
    """
}
