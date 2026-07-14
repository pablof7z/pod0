import Foundation
import os.log

// MARK: - ClipBoundaryResolver
//
// Asks the configured LLM (via OpenRouter / Ollama, same stack as wiki and AI
// chapters) to pick semantically meaningful start/end timestamps for a clip
// or shareable quote anchored at the playhead.
//
// The big idea is asymmetry: when a user taps "clip" or "share quote", they
// nearly always tap a few seconds *after* the moment of interest. So the
// window we send the LLM leans backward (`lookbackSeconds` >> `leadSeconds`)
// and the system prompt makes that bias explicit. Centering on T reproduces
// the bug the user complained about — mid-sentence quotes, half-captured
// thoughts.
//
// Output shape (JSON):
//
//   { "startSeconds": <float>, "endSeconds": <float>,
//     "quotedText": "<verbatim>", "speakerLabel": "<optional>" }
//
// Boundaries are validated against transcript span before the value is
// returned. Caller is responsible for fallback when this returns nil
// (no transcript, no key, network failure, malformed response).

@MainActor
final class ClipBoundaryResolver {

    // MARK: Singleton

    static let shared = ClipBoundaryResolver()

    // MARK: Logger

    nonisolated private static let logger = Logger.app("ClipBoundaryResolver")

    // MARK: Tunables

    enum Intent: Sendable {
        case clip   // 30–60s target span; full anecdote / answer.
        case quote  // 10–25s target span; one coherent quotable thought.
    }

    /// How much transcript before the tap to include in the LLM window.
    /// Big enough to reach back past the moment the user reacted to.
    static let lookbackSeconds: TimeInterval = 90
    /// How much transcript after the tap to include. A small forward grace
    /// so the LLM can capture the tail of a thought the speaker was still
    /// finishing when the user tapped.
    static let leadSeconds: TimeInterval = 15
    /// Cost-ledger feature key. Literal here (rather than on `CostFeature`)
    /// so the feature stays self-contained; ledger falls back to the raw
    /// key in its display name lookup.
    static let costFeatureKey: String = "clip.boundary.resolve"

    // MARK: Dependencies

    /// Client factory — overridable so tests can inject a stubbed client.
    /// Returning `nil` signals "no usable client right now" (no API key) and
    /// the resolver bails to nil without an LLM round-trip.
    var clientFactory: (LLMModelReference) -> WikiOpenRouterClient? = ClipBoundaryResolver.defaultClientFactory

    private init() {}

    // MARK: - Result

    struct ResolvedBoundaries: Sendable, Hashable {
        let startSeconds: TimeInterval
        let endSeconds: TimeInterval
        /// LLM-quoted text spanning [startSeconds, endSeconds]. Verbatim
        /// from the transcript window — used for the quote card body and
        /// the clip's frozen `transcriptText`.
        let quotedText: String
        /// Speaker UUID resolved from the dominant overlapping segment.
        /// `nil` when the resolved range crosses speakers (mixed run).
        let speakerID: UUID?
        /// Speaker display name returned by the LLM. Kept for tracing /
        /// debug surfaces; the source of truth for UI is `speakerID`.
        let speakerLabel: String?
    }

    // MARK: - API

    /// Resolve refined boundaries for the given `intent` anchored at
    /// `playheadSeconds`. Returns `nil` when there's nothing usable — no
    /// API key, transcript window too thin, malformed response, or LLM
    /// boundaries that don't validate.
    func resolveBoundaries(
        transcript: Transcript,
        playheadSeconds: TimeInterval,
        intent: Intent,
        modelID: String
    ) async -> ResolvedBoundaries? {
        let window = transcriptWindow(
            transcript: transcript,
            playheadSeconds: playheadSeconds
        )
        guard !window.isEmpty else {
            Self.logger.notice("resolveBoundaries: empty window around \(playheadSeconds, privacy: .public)s")
            return nil
        }
        let modelReference = LLMModelReference(storedID: modelID)
        guard let client = clientFactory(modelReference) else {
            Self.logger.info("resolveBoundaries: no client (likely no API key)")
            return nil
        }
        let userPrompt = userPrompt(
            window: window,
            playheadSeconds: playheadSeconds,
            intent: intent
        )
        let raw: String
        do {
            raw = try await client.compile(
                systemPrompt: Self.systemPrompt(for: intent),
                userPrompt: userPrompt,
                feature: Self.costFeatureKey
            )
        } catch {
            Self.logger.error("resolveBoundaries: LLM failed: \(String(describing: error), privacy: .public)")
            return nil
        }
        return parse(raw, transcript: transcript)
    }

    // MARK: - Window

    /// Slice of `[playhead - lookback, playhead + lead]` segments, returned
    /// in order. Asymmetric on purpose — see file-header comment.
    private func transcriptWindow(
        transcript: Transcript,
        playheadSeconds: TimeInterval
    ) -> [Segment] {
        let lo = max(0, playheadSeconds - Self.lookbackSeconds)
        let hi = playheadSeconds + Self.leadSeconds
        return transcript.segments.filter { seg in
            seg.end >= lo && seg.start <= hi
        }
    }

    // MARK: - Prompting

    private static func systemPrompt(for intent: Intent) -> String {
        let target: String
        switch intent {
        case .clip:
            target = "Target span: 30 to 60 seconds. Capture the full anecdote, answer, or argument."
        case .quote:
            target = "Target span: 10 to 25 seconds. Capture ONE coherent quotable thought — a complete sentence or two."
        }
        return """
        You pick semantic start/end timestamps for excerpting a podcast moment.

        Context: the user tapped at time T. Users almost always tap a few seconds \
        AFTER the moment of interest, while reacting to what was just said. Bias \
        backward in time. The interesting moment is likely 3-30 seconds BEFORE T.

        \(target)
        Boundaries must align with sentence breaks — never start or end mid-sentence. \
        Do not include ad reads, host station-IDs, or filler ("uh", "you know") at \
        the boundaries. Prefer a clean opening line.

        Respond with ONLY this JSON object — no prose, no markdown fences:
        { "startSeconds": <number>, "endSeconds": <number>, \
        "quotedText": "<verbatim text between start and end>", \
        "speakerLabel": "<optional, single speaker if the span is one speaker>" }

        Rules:
          - startSeconds and endSeconds must be inside the transcript window provided.
          - endSeconds must be greater than startSeconds.
          - quotedText must be verbatim from the transcript, no paraphrasing.
        """
    }

    private func userPrompt(
        window: [Segment],
        playheadSeconds: TimeInterval,
        intent _: Intent
    ) -> String {
        let lines = window.map { seg -> String in
            // [start.0s -> end.0s] text. Times are floats with one decimal —
            // enough resolution for clean boundaries without flooding tokens.
            let s = String(format: "%.1f", seg.start)
            let e = String(format: "%.1f", seg.end)
            let cleaned = seg.text.trimmingCharacters(in: .whitespacesAndNewlines)
            return "[\(s)s -> \(e)s] \(cleaned)"
        }
        let tapMarker = String(format: "User tapped at T = %.1f seconds.", playheadSeconds)
        return """
        \(tapMarker)
        Transcript window (timestamped segments, in order):
        \(lines.joined(separator: "\n"))
        """
    }

    // MARK: - Parsing

    /// Decode the LLM JSON, validate within transcript bounds, resolve the
    /// speaker UUID by majority overlap. Returns `nil` for any failure
    /// (malformed JSON, swapped/zero range, out-of-bounds).
    func parse(_ raw: String, transcript: Transcript) -> ResolvedBoundaries? {
        guard let data = raw.data(using: .utf8) else { return nil }
        struct Payload: Decodable {
            let startSeconds: Double
            let endSeconds: Double
            let quotedText: String?
            let speakerLabel: String?
        }
        let payload: Payload
        do {
            payload = try JSONDecoder().decode(Payload.self, from: data)
        } catch {
            Self.logger.notice("parse: malformed JSON (\(raw.prefix(120), privacy: .public))")
            return nil
        }
        guard let firstStart = transcript.segments.first?.start,
              let lastEnd = transcript.segments.last?.end else {
            return nil
        }
        let start = max(firstStart, payload.startSeconds)
        let end = min(lastEnd, payload.endSeconds)
        guard end - start > 0.5 else {
            Self.logger.notice("parse: invalid range (\(payload.startSeconds, privacy: .public)..\(payload.endSeconds, privacy: .public))")
            return nil
        }
        let speakerID = dominantSpeaker(transcript: transcript, start: start, end: end)
        let text = (payload.quotedText?.trimmingCharacters(in: .whitespacesAndNewlines)).flatMap {
            $0.isEmpty ? nil : $0
        } ?? fallbackText(transcript: transcript, start: start, end: end)
        return ResolvedBoundaries(
            startSeconds: start,
            endSeconds: end,
            quotedText: text,
            speakerID: speakerID,
            speakerLabel: payload.speakerLabel
        )
    }

    /// Speaker whose overlap with `[start, end]` is largest. Returns `nil`
    /// when the span has no single-speaker majority (>= 65% of duration).
    private func dominantSpeaker(transcript: Transcript, start: TimeInterval, end: TimeInterval) -> UUID? {
        var tally: [UUID: TimeInterval] = [:]
        for seg in transcript.segments where seg.end >= start && seg.start <= end {
            guard let sid = seg.speakerID else { continue }
            let overlap = min(seg.end, end) - max(seg.start, start)
            guard overlap > 0 else { continue }
            tally[sid, default: 0] += overlap
        }
        let total = tally.values.reduce(0, +)
        guard total > 0, let top = tally.max(by: { $0.value < $1.value }) else { return nil }
        return top.value / total >= 0.65 ? top.key : nil
    }

    /// Verbatim text reconstructed from overlapping segments — used when the
    /// LLM omits `quotedText` or returns an empty value.
    private func fallbackText(transcript: Transcript, start: TimeInterval, end: TimeInterval) -> String {
        transcript.segments
            .filter { $0.end > start && $0.start < end }
            .map(\.text)
            .joined(separator: " ")
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }
}

// MARK: - Default client factory

private extension ClipBoundaryResolver {
    static let defaultClientFactory: (LLMModelReference) -> WikiOpenRouterClient? = { modelReference in
        do {
            guard let key = try LLMProviderCredentialResolver.apiKey(for: modelReference.provider),
                  !key.isEmpty else {
                return nil
            }
            return WikiOpenRouterClient.live(apiKey: key, model: modelReference.storedID)
        } catch {
            return nil
        }
    }
}
