import Foundation

// MARK: - AgentPicksPrompt
//
// Prompt construction + JSON-response parsing for `AgentPicksService`. Lives
// next to the service but in its own file so prompt iteration doesn't churn
// the service's diff and the file-length cap stays intact.

enum AgentPicksPrompt {

    /// Base system instruction shared across every editorial framing. The
    /// per-category preamble is prepended in `systemInstruction(for:)` so
    /// the JSON contract + ordering rules stay in exactly one place.
    static let baseSystemInstruction = """
    You are a podcast curation assistant. Given the user's recent listening
    history, persistent memories, active threading topics, and the unplayed
    episodes across their subscriptions, pick episodes worth listening to next.

    Reply with STRICT JSON only — no prose, no markdown fences. Shape:

      {
        "hero": {
          "episode_id": "<UUID>",
          "reason": "<two-sentence reason>",
          "spoken_reason": "<2-3 sentence spoken variant, ≤30 words>"
        },
        "secondaries": [
          { "episode_id": "<UUID>", "reason": "<one-sentence reason>" },
          { "episode_id": "<UUID>", "reason": "<one-sentence reason>" }
        ]
      }

    Rules:
      • Pick exactly one hero and zero, one, or two secondaries.
      • Every episode_id MUST appear verbatim in the candidate list below.
      • Hero reason: 2 sentences (~140 chars total).
      • Hero spoken_reason: 2–3 sentences, ≤30 words, conversational tone
        because it will be read aloud by a TTS voice. Avoid bullet points,
        URLs, em-dashes, or anything that doesn't read naturally.
      • Secondary reason: 1 sentence (~80 chars). No spoken_reason needed
        for secondaries.
      • Reasons should reference what the user cares about (memories, topics,
        in-progress shows) — not just "you might like this."
      • Emit `hero` FIRST in your JSON output, then `secondaries`. This
        ordering lets the client surface the hero before the secondaries
        have arrived in the stream.
    """

    /// Backwards-compatible alias for the un-scoped prompt. Used by the
    /// "All Categories" path and by tests that don't care about framing.
    static var systemInstruction: String { baseSystemInstruction }

    /// Compose a system instruction with optional editorial framing for a
    /// category. The framing line tells the model what *kind* of pick the
    /// magazine section calls for; soft-pattern-matching on the name keeps
    /// the framing meaningful for first-class archetypes (Learning, News,
    /// Entertainment, Storytelling) while user-defined-feeling categories
    /// fall back to their own description.
    static func systemInstruction(for framing: CategoryFraming?) -> String {
        guard let framing else { return baseSystemInstruction }
        return """
        \(baseSystemInstruction)

        EDITORIAL FRAMING — \(framing.headerLabel)
        \(framing.guidance)
        Quote this framing in the hero `reason` so the user hears the
        magazine voice (e.g. "Because you're in \(framing.userFacingName) mode this morning…").
        """
    }

    static func build(
        inputs: AgentPicksInputs,
        framing: CategoryFraming? = nil
    ) -> String {
        var sections: [String] = []

        if let framing {
            // Surface the framing in the user message too — some providers
            // weight the user turn more heavily than the system role, so we
            // restate the editorial intent here rather than relying on the
            // system instruction alone.
            sections.append("## Magazine section\n\(framing.userMessageBlock)")
        }

        if !inputs.inProgress.isEmpty {
            let lines = inputs.inProgress.prefix(5).map {
                let show = inputs.subscriptionTitles[$0.podcastID] ?? "?"
                return "- \(show): \($0.title)"
            }.joined(separator: "\n")
            sections.append("## In-progress\n\(lines)")
        }

        if !inputs.memorySnippets.isEmpty {
            let lines = inputs.memorySnippets.prefix(8).map { "- \($0)" }.joined(separator: "\n")
            sections.append("## Memories\n\(lines)")
        }

        if !inputs.topicNames.isEmpty {
            let lines = inputs.topicNames.map { "- \($0)" }.joined(separator: "\n")
            sections.append("## Active topics\n\(lines)")
        }

        // Cap at 30 candidates so the prompt stays under a reasonable token
        // budget — picks are over the freshest few, not the whole back catalog.
        let candidates = inputs.unplayed.prefix(30).map { ep -> String in
            let show = inputs.subscriptionTitles[ep.podcastID] ?? "?"
            let dur = ep.duration.map { " (\(Int($0/60)) min)" } ?? ""
            return "- id=\(ep.id.uuidString) — \(show): \(ep.title)\(dur)"
        }.joined(separator: "\n")
        sections.append("## Candidate episodes (use ids verbatim)\n\(candidates)")

        sections.append("Reply with the JSON object now.")
        return sections.joined(separator: "\n\n")
    }

    /// Parse the model's reply. Tolerant: strips markdown fences, finds
    /// the first balanced JSON object, validates each `episode_id` against
    /// the candidate set, and silently drops malformed entries.
    static func parse(_ raw: String, knownEpisodeIDs: Set<UUID>) -> [HomeAgentPick] {
        guard let data = extractJSONData(from: raw) else { return [] }
        guard let dict = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            return []
        }
        var picks: [HomeAgentPick] = []
        if let hero = dict["hero"] as? [String: Any],
           let idStr = hero["episode_id"] as? String,
           let id = UUID(uuidString: idStr),
           knownEpisodeIDs.contains(id) {
            let reason = (hero["reason"] as? String) ?? ""
            let spoken = (hero["spoken_reason"] as? String) ?? ""
            picks.append(HomeAgentPick(
                episodeID: id,
                rationale: reason,
                spokenRationale: spoken,
                isHero: true
            ))
        }
        if let seconds = dict["secondaries"] as? [[String: Any]] {
            for entry in seconds.prefix(2) {
                guard let idStr = entry["episode_id"] as? String,
                      let id = UUID(uuidString: idStr),
                      knownEpisodeIDs.contains(id) else { continue }
                let reason = (entry["reason"] as? String) ?? ""
                let spoken = (entry["spoken_reason"] as? String) ?? ""
                picks.append(HomeAgentPick(
                    episodeID: id,
                    rationale: reason,
                    spokenRationale: spoken,
                    isHero: false
                ))
            }
        }
        return picks
    }

    // MARK: - Category framing

    /// Editorial framing for one Home magazine section. Built from a
    /// `PodcastCategory` (or omitted entirely for the All-Categories path).
    /// The archetype detection is deliberately a soft pattern match —
    /// users may have user-named categories ("Cosy nightcaps") for which
    /// the description is the only signal, so when no archetype matches
    /// we still surface the description verbatim to the model.
    struct CategoryFraming: Equatable, Sendable {
        let userFacingName: String
        let headerLabel: String
        let guidance: String
        let userMessageBlock: String

        /// Build a framing for `category`. Returns `nil` only when the
        /// category has no name and no description — anything else carries
        /// at least the description as guidance.
        static func make(from category: PodcastCategory) -> CategoryFraming? {
            let name = category.name.trimmingCharacters(in: .whitespacesAndNewlines)
            let description = category.description.trimmingCharacters(in: .whitespacesAndNewlines)
            if name.isEmpty && description.isEmpty { return nil }

            let archetype = Archetype.detect(name: name)
            let guidance: String
            switch archetype {
            case .learning:
                guidance = """
                The user is in LEARNING mode. Pick episodes that build on what
                they have been studying — surface continuations, contradictions,
                or intermediate-level material that pushes their understanding
                forward. Avoid pure entertainment picks here.
                """
            case .entertainment:
                guidance = """
                The user is in ENTERTAINMENT mode. Pick episodes that match
                the storytelling tone they have been enjoying — long-form
                interviews, narrative pieces, host-driven shows with personality.
                Avoid dense lecture-style content here.
                """
            case .news:
                guidance = """
                The user is in NEWS mode. Pick the freshest episode from the
                most active subscription in the section as the hero, then add
                one wildcard from a less-listened show in the section so the
                briefing isn't monotonous. Prefer episodes published in the
                last 24 hours.
                """
            case .storytelling:
                guidance = """
                The user is in STORYTELLING mode. Pick narrative-driven
                episodes — character-led arcs, reported pieces, audio essays.
                Avoid news round-ups and explainers; lean into voice + mood.
                """
            case .custom:
                let descriptionLine = description.isEmpty
                    ? "(no description provided — derive intent from the category name alone.)"
                    : "Description (use this to derive editorial intent): \(description)"
                guidance = """
                The user defined this section themselves. Match the editorial
                intent below.
                \(descriptionLine)
                """
            }

            let header = name.isEmpty
                ? archetype.fallbackHeader
                : name.uppercased()
            let userBlock: String
            if description.isEmpty {
                userBlock = "Section: \(name.isEmpty ? archetype.fallbackHeader : name)."
            } else {
                userBlock = "Section: \(name.isEmpty ? archetype.fallbackHeader : name).\nDescription: \(description)"
            }
            return CategoryFraming(
                userFacingName: name.isEmpty ? archetype.fallbackHeader : name,
                headerLabel: header,
                guidance: guidance,
                userMessageBlock: userBlock
            )
        }

        /// Soft archetype classifier. Pattern-match on lowercased name
        /// keywords; falls through to `.custom` when nothing matches so
        /// the description still gets surfaced.
        enum Archetype: Equatable {
            case learning
            case entertainment
            case news
            case storytelling
            case custom

            var fallbackHeader: String {
                switch self {
                case .learning:      return "LEARNING"
                case .entertainment: return "ENTERTAINMENT"
                case .news:          return "NEWS"
                case .storytelling:  return "STORYTELLING"
                case .custom:        return "CATEGORY"
                }
            }

            static func detect(name: String) -> Archetype {
                let lower = name.lowercased()
                // Order matters — "news" is short enough to false-match
                // "newsletter" if checked too aggressively, but the
                // canonicalised category names from the LLM categorizer
                // tend to use whole words.
                if lower.contains("learn") || lower.contains("educat") || lower.contains("course") || lower.contains("deep dive") {
                    return .learning
                }
                if lower.contains("news") || lower.contains("daily") || lower.contains("brief") || lower.contains("politics") {
                    return .news
                }
                if lower.contains("story") || lower.contains("narrative") || lower.contains("fiction") || lower.contains("memoir") {
                    return .storytelling
                }
                if lower.contains("entertain") || lower.contains("comedy") || lower.contains("interview") || lower.contains("conversation") || lower.contains("culture") {
                    return .entertainment
                }
                return .custom
            }
        }
    }

    /// Locate the first balanced `{...}` JSON object in `raw`. Some models
    /// wrap the JSON in markdown fences or add prose; the brace-matcher
    /// extracts just the object so the JSON deserializer never sees the
    /// surrounding noise.
    private static func extractJSONData(from raw: String) -> Data? {
        let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let firstBrace = trimmed.firstIndex(of: "{") else { return nil }
        var depth = 0
        var endIdx: String.Index?
        for idx in trimmed[firstBrace...].indices {
            let ch = trimmed[idx]
            if ch == "{" { depth += 1 }
            if ch == "}" {
                depth -= 1
                if depth == 0 { endIdx = idx; break }
            }
        }
        guard let endIdx else { return nil }
        return String(trimmed[firstBrace...endIdx]).data(using: .utf8)
    }
}
