import Foundation

// MARK: - AgentPicksStreamingParser
//
// Incrementally parses a *growing* JSON document of the shape
//
//   { "hero": {…}, "secondaries": [ {…}, {…} ] }
//
// emitting each inner `{…}` object as soon as its closing brace arrives so
// the view layer can render picks one at a time while the model is still
// generating. Tolerates leading markdown fences / preamble in the same way
// `AgentPicksPrompt.parse` does at end-of-stream — the parser locks onto
// the first outer `{` and ignores everything before it.
//
// Why this exists instead of re-running `JSONSerialization` every chunk:
//   • `onPartialContent` from `AgentOpenRouterClient` fires per SSE delta;
//     mid-stream the buffer is invalid JSON (unbalanced braces, half-
//     written strings), so `JSONSerialization` would throw on every chunk
//     until the very last one. We'd have to swallow noise and we'd never
//     emit early.
//   • A brace/string-aware single-pass scanner over the rolling buffer is
//     cheap and unambiguous given the shape we promise the model.
//
// The parser keeps a cursor `consumedCount` indicating how many characters
// of the buffer have already been turned into picks (i.e. emitted). Re-
// running `feed(_:)` with a longer buffer only re-scans the unconsumed tail,
// so each character of the response is examined exactly once across the
// stream's lifetime.

/// Pick category surfaced incrementally by the streaming parser.
enum AgentPickSlot: Sendable, Equatable {
    case hero
    case secondary
}

/// One pick emitted by `AgentPicksStreamingParser`.
struct AgentPicksStreamEvent: Sendable, Equatable {
    let slot: AgentPickSlot
    let episodeID: UUID
    let reason: String
    /// 2–3 sentence narrated variant the LLM may emit alongside `reason`
    /// (Task 3: voice-narrated rationale). Empty when the model omitted it.
    let spokenReason: String
}

/// Single-pass tolerant scanner. Not Sendable — created fresh per stream and
/// owned by the producing actor.
final class AgentPicksStreamingParser {

    // MARK: - State

    /// Outer-brace state. `nil` until we lock onto the first `{`.
    private var outerOpenIndex: Int?

    /// Number of characters from the buffer's *start* already accounted
    /// for. Includes the closing brace of any object already emitted plus
    /// everything before the outer `{`.
    private var consumedCount: Int = 0

    /// Current depth measured from the outer object (so the outer `{` itself
    /// sits at depth 1; the inner pick objects sit at depth 2 once entered).
    private var depth: Int = 0

    /// `true` while the scanner is sitting *inside* a JSON string literal.
    /// Braces inside strings must not be treated as structural.
    private var inString: Bool = false

    /// `true` immediately after a `\` inside a string — the next character
    /// is escaped and must not toggle `inString`.
    private var escapeNext: Bool = false

    /// The top-level key we are currently nested under. Drives whether an
    /// inner object becomes a `hero` or `secondary` event.
    private enum Section { case none, hero, secondaries }
    private var section: Section = .none

    /// Buffered top-level key while we wait for the `"` to close. The model
    /// is required to emit `"hero"` and `"secondaries"`; any other key is
    /// ignored.
    private var pendingKeyChars: [Character] = []
    private var inTopLevelKey: Bool = false

    /// Start index of the current inner object (depth 2) inside the buffer,
    /// if any. Used to slice out the substring for parsing once it closes.
    private var currentInnerOpenIndex: Int?

    // MARK: - Feed

    /// Feed the rolling accumulated buffer. Returns any new pick events
    /// produced. Safe to call with the *same* prefix re-scanned — we track
    /// `consumedCount` so unchanged characters are skipped.
    func feed(_ buffer: String, knownEpisodeIDs: Set<UUID>) -> [AgentPicksStreamEvent] {
        var events: [AgentPicksStreamEvent] = []

        // Walk by character index — `String.Index` would force us to
        // re-walk from `startIndex` every call. Going by `Int` over a
        // throwaway `Array<Character>` is O(n) per chunk; acceptable for
        // ≤4KB pick responses.
        let chars = Array(buffer)
        guard consumedCount < chars.count else { return events }

        // First, if we haven't found the outer `{` yet, scan for it. Once
        // found we record `outerOpenIndex` and bump depth to 1.
        var i = consumedCount
        if outerOpenIndex == nil {
            while i < chars.count {
                if chars[i] == "{" {
                    outerOpenIndex = i
                    depth = 1
                    i += 1
                    break
                }
                i += 1
            }
            // If still nothing, mark the prefix consumed and bail. (We
            // intentionally leave `consumedCount` unmoved so a later
            // `feed` call can still rediscover the `{` — the entire
            // pre-brace prefix is cheap to re-scan.)
            if outerOpenIndex == nil {
                consumedCount = i
                return events
            }
        }

        // Main scan.
        while i < chars.count {
            let ch = chars[i]

            // String handling — only structural characters outside strings count.
            if inString {
                if escapeNext {
                    escapeNext = false
                } else if ch == "\\" {
                    escapeNext = true
                } else if ch == "\"" {
                    inString = false
                    if inTopLevelKey {
                        let key = String(pendingKeyChars)
                        switch key {
                        case "hero":        section = .hero
                        case "secondaries": section = .secondaries
                        default:            break // ignored
                        }
                        inTopLevelKey = false
                        pendingKeyChars.removeAll(keepingCapacity: true)
                    }
                } else if inTopLevelKey {
                    pendingKeyChars.append(ch)
                }
                i += 1
                continue
            }

            switch ch {
            case "\"":
                inString = true
                // A `"` at depth 1 begins a top-level key.
                if depth == 1 {
                    inTopLevelKey = true
                    pendingKeyChars.removeAll(keepingCapacity: true)
                }

            case "{":
                depth += 1
                if depth == 2 {
                    currentInnerOpenIndex = i
                }

            case "}":
                if depth == 2, let start = currentInnerOpenIndex {
                    // Inner object just closed. Slice and parse.
                    let slice = String(chars[start...i])
                    if let event = makePick(from: slice, knownEpisodeIDs: knownEpisodeIDs) {
                        events.append(event)
                    }
                    currentInnerOpenIndex = nil
                }
                depth -= 1
                if depth == 0 {
                    // Outer object closed — stop scanning.
                    i += 1
                    consumedCount = i
                    return events
                }

            default:
                break
            }

            i += 1
        }

        consumedCount = i
        return events
    }

    // MARK: - Helpers

    /// Parse a single inner object slice. Tolerates trailing prose (e.g.
    /// `","`) — `JSONSerialization` ignores characters past the closing
    /// brace when given just the brace-balanced substring.
    private func makePick(from slice: String, knownEpisodeIDs: Set<UUID>) -> AgentPicksStreamEvent? {
        guard let data = slice.data(using: .utf8),
              let dict = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let idStr = dict["episode_id"] as? String,
              let id = UUID(uuidString: idStr),
              knownEpisodeIDs.contains(id) else { return nil }
        let reason = (dict["reason"] as? String) ?? ""
        let spoken = (dict["spoken_reason"] as? String) ?? ""
        let slot: AgentPickSlot = (section == .hero) ? .hero : .secondary
        return AgentPicksStreamEvent(
            slot: slot,
            episodeID: id,
            reason: reason,
            spokenReason: spoken
        )
    }
}
