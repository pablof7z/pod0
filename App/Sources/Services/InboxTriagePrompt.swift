import Foundation

// MARK: - InboxTriagePrompt
//
// Prompt construction and JSON-response parsing for `InboxTriageService`.
// Held in its own file so prompt iteration doesn't churn the service and
// the file-length cap stays intact.
//
// The triage prompt is a *per-episode* classifier: given the candidate
// list plus per-show engagement signals (how often the user actually
// finishes episodes from that show), the model decides for each
// episode whether it belongs in the user's Inbox (with a one-line
// "because …" reason) or should be silently archived. There is no
// review surface for archive decisions — by product brief, the user
// trusts the agent — so the prompt does not ask for an archive
// rationale.

enum InboxTriagePrompt {

    /// System instruction: defines the agent's role and the strict JSON
    /// contract. Kept terse so smaller models follow the schema reliably.
    static let systemInstruction = """
    You are the user's podcast inbox triage agent. The user has subscribed
    to several shows; new episodes arrive constantly. Your job: for each
    candidate episode, decide whether it belongs in the user's Inbox
    (worth listening to right now) or should be silently archived (skip —
    based on the user's listening history they are unlikely to play it).

    Use the per-show engagement signals to inform your decision:
      • Shows with a high finish-rate are signals the user values — lean
        toward Inbox for fresh episodes from those shows.
      • Shows with many unplayed episodes and few played episodes signal
        a neglected subscription — lean toward archive unless the
        specific episode looks unusually compelling.
      • Shows tagged `[newly subscribed — default to inbox]` have
        insufficient history to judge. The user just chose to follow
        them; respect that intent and surface their fresh episodes
        unless one is obvious filler (a re-run, a paid-tier promo,
        an obvious off-topic interlude).
      • Recency matters: episodes from the last 24h are more "current"
        than week-old material.

    Reply with STRICT JSON only — no prose, no markdown fences. Shape:

      {
        "decisions": [
          {
            "episode_id": "<UUID>",
            "decision": "inbox" | "archived",
            "reason": "<one-sentence reason, ~80 chars; only for inbox decisions>",
            "is_hero": true
          },
          …
        ]
      }

    Rules:
      • Every candidate episode below MUST appear exactly once in
        `decisions`. Do not drop any.
      • `episode_id` MUST match a candidate id verbatim.
      • For `inbox` decisions, include a one-sentence `reason` framed as
        what the user will find valuable ("Continues the thread on X you
        finished last week", "Fresh episode from a show you finish weekly").
      • For `archived` decisions, `reason` may be omitted (or empty
        string). The user will not see archive reasons.
      • Pick at most ONE hero across all your inbox decisions — the
        single episode the user should listen to first. Set
        `"is_hero": true` on that one decision; omit the field (or set
        false) on every other inbox decision. Archived decisions never
        get `is_hero`.
      • Be selective. A reasonable split is 20–40% Inbox / 60–80% archive
        when the candidate list has more than a handful of items. If
        every candidate looks worth surfacing, that's fine — but err on
        archive when the show's engagement is low.
    """

    /// Build the user-side prompt.
    static func build(input: InboxTriageInput) -> String {
        var sections: [String] = []

        if !input.engagement.isEmpty {
            let lines = input.engagement.map { snapshot -> String in
                let finished = snapshot.playedCount
                let unplayed = snapshot.unplayedCount
                let recency: String
                if let last = snapshot.lastPlayedAt {
                    let days = max(0, Int(Date().timeIntervalSince(last) / 86_400))
                    recency = days == 0 ? "today" : "\(days)d ago"
                } else {
                    recency = "never"
                }
                let suffix = snapshot.isNewlySubscribed ? " [newly subscribed — default to inbox]" : ""
                return "- \(snapshot.showTitle): finished \(finished), unplayed \(unplayed), last play \(recency)\(suffix)"
            }.joined(separator: "\n")
            sections.append("## Per-show engagement (last 20 episodes per show)\n\(lines)")
        }

        let candidateLines = input.candidates.map { ep -> String in
            let dur = ep.durationMinutes.map { " (\($0) min)" } ?? ""
            let age: String
            let daysOld = max(0, Int(Date().timeIntervalSince(ep.pubDate) / 86_400))
            age = daysOld == 0 ? "today" : "\(daysOld)d ago"
            return "- id=\(ep.id.uuidString) — \(ep.showTitle) · \(age)\(dur): \(ep.title)"
        }.joined(separator: "\n")
        sections.append("## Candidate episodes (decide for each)\n\(candidateLines)")

        sections.append("Reply with the JSON object now.")
        return sections.joined(separator: "\n\n")
    }

    /// Parse the model's reply. Tolerant: strips fences, extracts the
    /// first balanced JSON object, validates every `episode_id` against
    /// the candidate set, and silently drops malformed entries. Returns
    /// a dict so callers can quickly merge results back onto episodes.
    static func parse(
        _ raw: String,
        knownEpisodeIDs: Set<UUID>
    ) -> [UUID: ParsedDecision] {
        guard let data = extractJSONData(from: raw) else { return [:] }
        guard let dict = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            return [:]
        }
        guard let decisions = dict["decisions"] as? [[String: Any]] else { return [:] }

        var out: [UUID: ParsedDecision] = [:]
        var heroClaimed = false
        for entry in decisions {
            guard let idStr = entry["episode_id"] as? String,
                  let id = UUID(uuidString: idStr),
                  knownEpisodeIDs.contains(id),
                  let decisionRaw = entry["decision"] as? String else { continue }
            let lowered = decisionRaw.lowercased().trimmingCharacters(in: .whitespacesAndNewlines)
            switch lowered {
            case "inbox":
                let reason = ((entry["reason"] as? String) ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
                // First inbox entry claiming `is_hero` wins; later
                // claims are forced false so callers never have to pick
                // among competing heroes.
                let claimedHero = (entry["is_hero"] as? Bool) ?? false
                let isHero = claimedHero && !heroClaimed
                if isHero { heroClaimed = true }
                out[id] = .inbox(rationale: reason, isHero: isHero)
            case "archived", "archive":
                out[id] = .archived
            default:
                continue
            }
        }
        return out
    }

    /// Locate the first balanced `{...}` JSON object in `raw`. Models
    /// occasionally wrap the JSON in markdown fences or add prose; the
    /// brace-matcher extracts just the object.
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

// MARK: - Inputs / Outputs

/// One row in the candidate list the LLM sees. Carried separately from
/// `Episode` so the prompt builder can be tested without standing up the
/// whole store.
struct InboxTriageCandidate: Sendable, Hashable {
    let id: UUID
    let showTitle: String
    let title: String
    let pubDate: Date
    let durationMinutes: Int?
}

/// Per-show engagement snapshot summarising the user's behaviour on a
/// single subscription over the last N episodes. Drives the LLM's
/// "is this show worth surfacing?" instinct without the model needing
/// to do its own arithmetic on raw history.
struct InboxTriageShowEngagement: Sendable, Hashable {
    let podcastID: UUID
    let showTitle: String
    let playedCount: Int
    let unplayedCount: Int
    let lastPlayedAt: Date?
    /// `true` when total history (played + unplayed) is below the
    /// minimum signal threshold — i.e. the user just subscribed and the
    /// agent should NOT lean toward archive for this show on this pass.
    let isNewlySubscribed: Bool
}

/// Aggregated input passed to the prompt builder.
struct InboxTriageInput: Sendable {
    let candidates: [InboxTriageCandidate]
    let engagement: [InboxTriageShowEngagement]
}

/// One parsed decision from the LLM reply. `.inbox` carries the
/// one-line rationale plus an `isHero` flag (at most one hero per pass —
/// the parser enforces uniqueness); `.archived` carries no metadata
/// because the user never sees archive reasons.
enum ParsedDecision: Sendable, Hashable {
    case inbox(rationale: String, isHero: Bool)
    case archived
}
