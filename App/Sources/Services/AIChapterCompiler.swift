import Foundation
import os.log

// MARK: - AIChapterCompiler
//
// Asks the configured LLM (via OpenRouter / Ollama, same provider stack the
// wiki pipeline uses) to do three things in a single round trip from a ready
// transcript:
//
//   1. Synthesise 4–12 chapter boundaries when the episode has none yet.
//   2. Attach a 1–2 sentence summary to each chapter (the LLM's own boundaries
//      OR the publisher's existing chapters — both paths get summaries).
//   3. Mark ad-read spans so the player can auto-skip them
//      (`Settings.autoSkipAds`) and the chapter rail can stripe overlap.
//
// Persists chapters via `AppStateStore.setEpisodeChapters` (with `isAIGenerated`
// set only when boundaries came from the model) and ads via
// `setEpisodeAdSegments`.
//
// Design notes
//   • Idempotent — gated on `adSegments == nil`. Once the combined call has
//     run and persisted ads (even an empty array), the service no-ops. This
//     matches the previous `AdSegmentDetector` gate and ensures we don't
//     re-bill on every episode open.
//   • Forces `response_format: json_object` for parse stability.
//   • Two prompt branches: one for "no chapters yet" (produce chapters +
//     summaries + ads); one for "publisher chapters exist" (produce
//     summaries-by-index + ads, leave boundaries alone).
//   • Validates monotonic timestamps and clamps to the episode duration
//     before persisting; rejects malformed payloads silently.
//   • Stays read-only against the network — no Scribe / publisher fetches,
//     no chunk re-indexing.

@MainActor
final class AIChapterCompiler {

    // MARK: Singleton

    static let shared = AIChapterCompiler()

    // MARK: Logger

    nonisolated private static let logger = Logger.app("AIChapterCompiler")

    // MARK: Tunables

    /// Cap the transcript bytes we send so the prompt doesn't blow past the
    /// model's context window on long shows. ~28KB is comfortable for any
    /// modern model and still preserves enough structure for inference.
    static let maxTranscriptCharacters: Int = 28_000

    static let minChapters: Int = 4
    static let maxChapters: Int = 12

    /// Cost-ledger feature key. Lives as a literal here (rather than on
    /// `CostFeature`) so this feature stays self-contained — `CostLedger`
    /// accepts arbitrary feature strings and `displayName(for:)` falls back
    /// to the raw key when unrecognised.
    static let costFeatureKey: String = "ai.chapter.compile"

    // MARK: Dedup

    private var inFlight: Set<UUID> = []

    // MARK: API

    /// Run the combined chapter / summary / ad compile when (a) the
    /// transcript is `.ready` and (b) the episode doesn't already have
    /// cached `adSegments`. No-op otherwise. When publisher chapters already
    /// exist, the boundaries are kept and only summaries + ads are merged in.
    func compileIfNeeded(episodeID: UUID, store: AppStateStore) async {
        guard let episode = store.episode(id: episodeID) else { return }
        guard episode.adSegments == nil else { return }
        guard case .ready = episode.transcriptState else { return }
        guard !inFlight.contains(episodeID) else { return }
        inFlight.insert(episodeID)
        defer { inFlight.remove(episodeID) }

        guard let transcript = TranscriptStore.shared.load(episodeID: episodeID) else {
            Self.logger.notice(
                "compileIfNeeded(\(episodeID, privacy: .public)): transcript file missing"
            )
            return
        }
        let modelReference = LLMModelReference(storedID: store.state.settings.chapterCompilationModel)
        let apiKey: String
        do {
            guard let resolved = try LLMProviderCredentialResolver.apiKey(for: modelReference.provider),
                  !resolved.isEmpty else {
                Self.logger.info(
                    "compileIfNeeded(\(episodeID, privacy: .public)): no \(modelReference.provider.displayName, privacy: .public) key configured — skipping"
                )
                return
            }
            apiKey = resolved
        } catch {
            Self.logger.error(
                "compileIfNeeded(\(episodeID, privacy: .public)): credential resolve failed: \(String(describing: error), privacy: .public)"
            )
            return
        }

        let hasExistingChapters = (episode.chapters?.isEmpty == false)
        let systemPrompt = hasExistingChapters ? Self.systemPromptEnrichOnly : Self.systemPromptFull
        let userPrompt = hasExistingChapters
            ? enrichOnlyUserPrompt(transcript: transcript, episode: episode)
            : fullUserPrompt(transcript: transcript, episode: episode)

        let client = WikiOpenRouterClient.live(apiKey: apiKey, model: modelReference.storedID)
        let raw: String
        do {
            raw = try await client.compile(
                systemPrompt: systemPrompt,
                userPrompt: userPrompt,
                feature: Self.costFeatureKey
            )
        } catch {
            Self.logger.error(
                "compileIfNeeded(\(episodeID, privacy: .public)): LLM call failed: \(String(describing: error), privacy: .public)"
            )
            return
        }

        if hasExistingChapters, let existing = episode.chapters {
            let (summariesByIndex, ads) = parseEnrichOnly(raw, durationCap: episode.duration)
            let enriched = applySummaries(to: existing, indexed: summariesByIndex)
            store.setEpisodeChapters(episodeID, chapters: enriched)
            store.setEpisodeAdSegments(episodeID, segments: ads ?? [])
            Self.logger.info(
                "compileIfNeeded(\(episodeID, privacy: .public)): enriched \(existing.count, privacy: .public) publisher chapters, wrote \(ads?.count ?? 0, privacy: .public) ads"
            )
        } else {
            guard let parsed = parseFull(raw, durationCap: episode.duration) else {
                Self.logger.notice(
                    "compileIfNeeded(\(episodeID, privacy: .public)): payload rejected (\(raw.prefix(120), privacy: .public))"
                )
                // Still persist an empty ad-segment marker so we don't loop.
                store.setEpisodeAdSegments(episodeID, segments: [])
                return
            }
            store.setEpisodeChapters(episodeID, chapters: parsed.chapters)
            store.setEpisodeAdSegments(episodeID, segments: parsed.ads)
            Self.logger.info(
                "compileIfNeeded(\(episodeID, privacy: .public)): wrote \(parsed.chapters.count, privacy: .public) AI chapters and \(parsed.ads.count, privacy: .public) ads"
            )
        }
    }

    // MARK: - Prompting

    /// Prompt used when the episode has no chapters yet — the model produces
    /// chapter boundaries, summaries, and ad spans in one shot.
    private static let systemPromptFull: String = """
    You analyse podcast episode transcripts and return chapter boundaries, \
    chapter summaries, and advertisement spans in a single JSON response. \
    Always respond with ONLY this JSON object (no prose, no markdown fences):
    {
      "chapters": [
        { "start": <seconds>, "title": "<short title>", "summary": "<1-2 sentence summary>" }
      ],
      "ads": [
        { "start": <seconds>, "end": <seconds>, "kind": "preroll"|"midroll"|"postroll" }
      ]
    }
    Chapter rules:
      - Produce between 4 and 12 chapters total.
      - "start" is seconds from the beginning of the episode, integer or float.
      - The first chapter must start at 0.
      - Chapters must be strictly monotonic by "start".
      - Titles are short (max 6 words), descriptive, no quotes, no episode numbers.
      - "summary" is 1-2 sentences describing what the chapter covers.
      - Skip ad reads — don't create a chapter for them.
      - Prefer topic shifts over speaker changes.
    Ad rules:
      - Only mark spans that are clearly advertisements (host-read or pre-recorded sponsor copy).
      - Do NOT mark guest plugs, book recommendations, or off-topic asides.
      - "start"/"end" are seconds; "end" must be greater than "start".
      - Ranges must be non-overlapping and strictly increasing by "start".
      - "kind": "preroll" if before any topical content; "postroll" if after; otherwise "midroll".
      - Return an empty "ads" array if the episode has no ads.
    """

    /// Prompt used when the episode already has publisher chapters — the
    /// model only adds summaries (matched by index) and detects ads.
    private static let systemPromptEnrichOnly: String = """
    You analyse podcast episode transcripts. The episode already has chapter \
    boundaries supplied by the publisher (numbered below). Your job is to:
      1. Write a 1-2 sentence summary for each existing chapter.
      2. Identify advertisement spans inside the episode.
    Always respond with ONLY this JSON object (no prose, no markdown fences):
    {
      "summaries": [
        { "index": <int>, "summary": "<1-2 sentence summary>" }
      ],
      "ads": [
        { "start": <seconds>, "end": <seconds>, "kind": "preroll"|"midroll"|"postroll" }
      ]
    }
    Summary rules:
      - One entry per chapter; "index" is the chapter number from the list below.
      - 1-2 sentences describing what the chapter covers.
      - Do NOT change titles or invent new chapters.
    Ad rules:
      - Only mark spans that are clearly advertisements (host-read or pre-recorded sponsor copy).
      - Do NOT mark guest plugs, book recommendations, or off-topic asides.
      - "start"/"end" are seconds; "end" must be greater than "start".
      - Ranges must be non-overlapping and strictly increasing by "start".
      - "kind": "preroll" if before any topical content; "postroll" if after; otherwise "midroll".
      - Return an empty "ads" array if the episode has no ads.
    """

    private func fullUserPrompt(transcript: Transcript, episode: Episode) -> String {
        let body = transcriptBody(transcript)
        let durationLine = episode.duration.map { "Episode duration: \(Int($0)) seconds.\n" } ?? ""
        return """
        \(durationLine)Title: \(episode.title)
        Transcript (timestamped):
        \(body)
        """
    }

    private func enrichOnlyUserPrompt(transcript: Transcript, episode: Episode) -> String {
        let body = transcriptBody(transcript)
        let durationLine = episode.duration.map { "Episode duration: \(Int($0)) seconds.\n" } ?? ""
        let chapterLines = (episode.chapters ?? []).enumerated().map { idx, ch in
            "[\(idx)] \(Int(ch.startTime))s — \(ch.title)"
        }.joined(separator: "\n")
        return """
        \(durationLine)Title: \(episode.title)
        Existing chapters (use these exact indices in your "summaries" output):
        \(chapterLines)
        Transcript (timestamped):
        \(body)
        """
    }

    private func transcriptBody(_ transcript: Transcript) -> String {
        let lines = transcript.segments.map { seg -> String in
            let ts = Int(seg.start.rounded())
            let cleaned = seg.text.trimmingCharacters(in: .whitespacesAndNewlines)
            return "[\(ts)s] \(cleaned)"
        }
        var body = lines.joined(separator: "\n")
        if body.count > Self.maxTranscriptCharacters {
            body = String(body.prefix(Self.maxTranscriptCharacters))
        }
        return body
    }

    // MARK: - Parsing

    struct FullParseResult {
        let chapters: [Episode.Chapter]
        let ads: [Episode.AdSegment]
    }

    private struct AdItem: Decodable {
        // Accept both `start`/`end` (full prompt) and `start_seconds`/`end_seconds`
        // (legacy detector prompt shape) so we tolerate either output style.
        let start: Double?
        let end: Double?
        let start_seconds: Double?
        let end_seconds: Double?
        let kind: String?

        var resolvedStart: Double? { start ?? start_seconds }
        var resolvedEnd: Double? { end ?? end_seconds }
    }

    /// Decodes the full payload (chapters + summaries + ads). Returns `nil`
    /// when the chapter list is unusable; ad parsing failure alone returns
    /// an empty ads array instead of failing the whole response.
    func parseFull(_ raw: String, durationCap: TimeInterval?) -> FullParseResult? {
        guard let data = raw.data(using: .utf8) else { return nil }
        struct Payload: Decodable {
            struct ChapterItem: Decodable {
                let start: Double
                let title: String
                let summary: String?
            }
            let chapters: [ChapterItem]
            let ads: [AdItem]?
        }
        let payload: Payload
        do {
            payload = try JSONDecoder().decode(Payload.self, from: data)
        } catch {
            return nil
        }

        var prev: Double = -1
        var chapters: [Episode.Chapter] = []
        for item in payload.chapters {
            let title = item.title.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !title.isEmpty else { continue }
            let cap = durationCap ?? Double.greatestFiniteMagnitude
            let clamped = max(0, min(item.start, cap))
            guard clamped > prev else { continue }
            prev = clamped
            let summary = item.summary?.trimmingCharacters(in: .whitespacesAndNewlines)
            chapters.append(Episode.Chapter(
                startTime: clamped,
                title: title,
                isAIGenerated: true,
                summary: (summary?.isEmpty == false) ? summary : nil
            ))
            if chapters.count >= Self.maxChapters { break }
        }
        guard chapters.count >= Self.minChapters else { return nil }
        if chapters.first!.startTime > 0 {
            var first = chapters[0]
            first.startTime = 0
            chapters[0] = first
        }
        let ads = validateAds(payload.ads ?? [], durationCap: durationCap)
        return FullParseResult(chapters: chapters, ads: ads)
    }

    /// Decodes the enrich-only payload (summaries-by-index + ads). Returns a
    /// tuple of `(summariesByIndex, ads)`. `ads == nil` only on parse failure;
    /// otherwise an empty array means "ran, found no ads."
    func parseEnrichOnly(
        _ raw: String,
        durationCap: TimeInterval?
    ) -> (summaries: [Int: String], ads: [Episode.AdSegment]?) {
        guard let data = raw.data(using: .utf8) else { return ([:], nil) }
        struct Payload: Decodable {
            struct SummaryItem: Decodable {
                let index: Int
                let summary: String
            }
            let summaries: [SummaryItem]?
            let ads: [AdItem]?
        }
        let payload: Payload
        do {
            payload = try JSONDecoder().decode(Payload.self, from: data)
        } catch {
            return ([:], nil)
        }
        var map: [Int: String] = [:]
        for item in payload.summaries ?? [] {
            let s = item.summary.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !s.isEmpty else { continue }
            map[item.index] = s
        }
        let ads = validateAds(payload.ads ?? [], durationCap: durationCap)
        return (map, ads)
    }

    private func validateAds(
        _ items: [AdItem],
        durationCap: TimeInterval?
    ) -> [Episode.AdSegment] {
        let cap = durationCap ?? Double.greatestFiniteMagnitude
        var prevEnd: Double = -1
        var result: [Episode.AdSegment] = []
        for item in items {
            guard let s = item.resolvedStart, let e = item.resolvedEnd else { continue }
            let start = max(0, min(s, cap))
            let end = max(0, min(e, cap))
            guard end > start else { continue }
            guard start >= prevEnd else { continue }
            let kind = Episode.AdKind(rawValue: item.kind ?? "midroll") ?? .midroll
            result.append(Episode.AdSegment(start: start, end: end, kind: kind))
            prevEnd = end
        }
        return result
    }

    /// Apply LLM-generated summaries onto existing chapters in-place by index.
    /// Returns a new chapter array; chapters whose index has no summary are
    /// left unchanged.
    func applySummaries(
        to chapters: [Episode.Chapter],
        indexed map: [Int: String]
    ) -> [Episode.Chapter] {
        guard !map.isEmpty else { return chapters }
        var result = chapters
        for (idx, summary) in map {
            guard idx >= 0, idx < result.count else { continue }
            result[idx].summary = summary
        }
        return result
    }
}

// MARK: - Chapter overlap helper
//
// Moved here from the (now deleted) AdSegmentDetector. `PlayerChaptersScrollView`
// uses it to flag ad-overlapping chapters with the amber stripe.

extension Episode.Chapter {
    /// `true` when this chapter's `[startTime, effectiveEnd)` window
    /// overlaps any of `adSegments`. `chapters` is the full list so the
    /// helper can resolve an implicit `endTime` from the next chapter's
    /// `startTime` when this chapter has no explicit `endTime`. For the last
    /// chapter we treat the end as `+∞` — any ad after `startTime` overlaps.
    func overlapsAd(
        in chapters: [Episode.Chapter],
        adSegments: [Episode.AdSegment]
    ) -> Bool {
        guard !adSegments.isEmpty else { return false }
        let effectiveEnd: TimeInterval
        if let end = endTime {
            effectiveEnd = end
        } else if let idx = chapters.firstIndex(where: { $0.id == id }),
                  chapters.index(after: idx) < chapters.endIndex {
            effectiveEnd = chapters[chapters.index(after: idx)].startTime
        } else {
            effectiveEnd = .greatestFiniteMagnitude
        }
        return adSegments.contains { ad in
            ad.start < effectiveEnd && ad.end > startTime
        }
    }
}
