import Foundation

// MARK: - BriefingComposing protocol

/// The composer's public surface — extracted as a protocol so Lane 10's
/// `generate_briefing` agent tool can be unit-tested with a mock composer
/// without standing up the full RAG/TTS/storage stack.
protocol BriefingComposing: Sendable {
    func compose(
        request: BriefingRequest,
        progress: @escaping @Sendable (BriefingComposeProgress) -> Void
    ) async throws -> BriefingComposeResult
}

/// Real-time signal emitted as the composer moves through its pipeline.
/// Drives the *Composing your briefing* surface (W6 — generation in progress).
enum BriefingComposeProgress: Sendable, Hashable {
    case selectedEpisodes(count: Int)
    case draftedSegments(count: Int)
    case synthesizingVoice(segmentIndex: Int, total: Int)
    case stitchingQuotes
    case finished
}

/// What `BriefingComposer.compose` returns. The script is the persisted data;
/// the tracks are the ordered playable units the player engine consumes.
struct BriefingComposeResult: Sendable {
    var script: BriefingScript
    var tracks: [BriefingTrack]
    /// URL of the stitched .m4a — same as `BriefingStorage.audioURL(id:)`.
    var stitchedAudioURL: URL
}

// MARK: - BriefingComposer

/// Produces synthesized audio briefings.
///
/// Pipeline (UX-08 §1, §3):
///  1. Gather candidate episodes / clips / wikis via `BriefingRAGSearchProtocol`
///     (Lane 6) and `BriefingWikiStorageProtocol` (Lane 7).
///  2. Compose script via the selected LLM provider. Fixture scripts are
///     available only when the caller explicitly opts into test fallback.
///  3. Segment the script into `BriefingSegment[]`.
///  4. Stitch audio: each segment produces TTS tracks (synthesised via
///     `TTSProtocol` — Lane 8) interleaved with original-audio quote tracks
///     trimmed from `Episode.enclosureURL`.
///  5. Persist `BriefingScript` (json) and the stitched .m4a via `BriefingStorage`.
final class BriefingComposer: BriefingComposing, @unchecked Sendable {

    // MARK: Dependencies

    let rag: BriefingRAGSearchProtocol
    let wiki: BriefingWikiStorageProtocol
    let tts: TTSProtocol
    let storage: BriefingStorage
    /// OpenRouter API key. When `nil`, the composer skips the network call and
    /// asks the provider resolver for the selected model's credential.
    let apiKey: String?
    /// Model identifier for OpenRouter. Defaults to a balanced model; callers
    /// override at construction time.
    let model: String
    /// TTS voice id. Empty string = provider default.
    let voiceID: String
    /// Tests/previews may opt into deterministic scripts. Production keeps
    /// provider failures visible instead of fabricating a briefing.
    let allowFixtureFallback: Bool

    /// Production default: wires live RAG + WikiStorage + ElevenLabs TTS.
    /// Marked `@MainActor` because the singletons are main-actor isolated;
    /// the only production call site (`BriefingsViewModel`) is already on
    /// the main actor, and the explicit `init(rag:wiki:tts:storage:...)`
    /// overload below stays nonisolated for tests and DI.
    @MainActor
    convenience init(
        storage: BriefingStorage,
        apiKey: String? = nil,
        model: String = "openai/gpt-4o-mini",
        voiceID: String = "",
        allowFixtureFallback: Bool = false
    ) {
        self.init(
            rag: RAGService.shared.briefingRAG,
            wiki: WikiStorage.shared,
            tts: ElevenLabsBriefingTTS(),
            storage: storage,
            apiKey: apiKey,
            model: model,
            voiceID: voiceID,
            allowFixtureFallback: allowFixtureFallback
        )
    }

    init(
        rag: BriefingRAGSearchProtocol,
        wiki: BriefingWikiStorageProtocol,
        tts: TTSProtocol,
        storage: BriefingStorage,
        apiKey: String? = nil,
        model: String = "openai/gpt-4o-mini",
        voiceID: String = "",
        allowFixtureFallback: Bool = false
    ) {
        self.rag = rag
        self.wiki = wiki
        self.tts = tts
        self.storage = storage
        self.apiKey = apiKey
        self.model = model
        self.voiceID = voiceID
        self.allowFixtureFallback = allowFixtureFallback
    }

    // MARK: Compose

    func compose(
        request: BriefingRequest,
        progress: @escaping @Sendable (BriefingComposeProgress) -> Void
    ) async throws -> BriefingComposeResult {
        try validate(request)

        // 1) Gather candidates.
        let query = effectiveQuery(for: request)
        let candidates = try await rag.search(query: query, scope: request.scope, limit: 12)
        guard !candidates.isEmpty else {
            throw BriefingComposerError.noEvidence(scope: request.scope)
        }
        progress(.selectedEpisodes(count: countDistinctEpisodes(candidates)))
        let wikiTitles = try await relevantWikiTitles(for: request)

        // 2) LLM-compose. Fixture fallback is opt-in for tests/previews only.
        let llmScript: LLMScriptDraft
        do {
            llmScript = try await composeViaLLM(
                request: request,
                candidates: candidates,
                wikiTitles: wikiTitles
            )
        } catch {
            guard allowFixtureFallback else { throw error }
            llmScript = BriefingFixtureScript.make(request: request, candidates: candidates)
        }
        progress(.draftedSegments(count: llmScript.segments.count))

        // 3) Synthesise per-segment narration + assemble tracks.
        let segmentsDir = try storage.segmentsDirectory(id: request.id)
        var tracks: [BriefingTrack] = []
        var totalDuration: TimeInterval = 0
        for (index, segment) in llmScript.segments.enumerated() {
            progress(.synthesizingVoice(segmentIndex: index, total: llmScript.segments.count))
            let segmentTracks = try await synthesizeSegment(
                segment: segment,
                candidates: candidates,
                segmentsDir: segmentsDir
            )
            tracks.append(contentsOf: segmentTracks)
            totalDuration += segmentTracks.reduce(0) { $0 + $1.durationSeconds }
        }

        // 4) Stitch into one m4a.
        progress(.stitchingQuotes)
        let stitchedURL = storage.audioURL(id: request.id)
        let realDuration = try await BriefingAudioStitcher.stitch(
            tracks: tracks,
            outputURL: stitchedURL
        )

        // 5) Persist script.
        let script = BriefingScript(
            id: request.id,
            title: llmScript.title,
            subtitle: llmScript.subtitle,
            request: request,
            segments: llmScript.segments,
            sources: aggregateSources(from: llmScript.segments),
            generatedAt: Date(),
            totalDurationSeconds: realDuration > 0 ? realDuration : totalDuration,
            isPartial: false
        )
        try storage.save(script)

        progress(.finished)
        return BriefingComposeResult(
            script: script,
            tracks: tracks,
            stitchedAudioURL: stitchedURL
        )
    }

    // MARK: - Private helpers

    private func validate(_ request: BriefingRequest) throws {
        switch request.scope {
        case .thisShow:
            throw BriefingComposerError.unsupportedScope(
                "This-show briefings need a specific show id; the current compose request does not carry one."
            )
        case .thisTopic where request.freeformQuery.trimmedOrEmpty.isEmpty:
            throw BriefingComposerError.missingTopic
        case .mySubscriptions, .thisTopic, .thisWeek:
            return
        }
    }

    private func effectiveQuery(for request: BriefingRequest) -> String {
        if let q = request.freeformQuery, !q.isEmpty { return q }
        switch request.style {
        case .morning:            return "today's most important threads across my podcasts"
        case .weeklyTLDR:         return "this week in podcasts I subscribe to"
        case .catchUpOnShow:      return "what I missed on this show"
        case .topicAcrossLibrary: return request.freeformQuery ?? "topic across library"
        }
    }

    private func countDistinctEpisodes(_ candidates: [RAGCandidate]) -> Int {
        Set(candidates.compactMap(\.episodeID)).count
    }

    private func relevantWikiTitles(for request: BriefingRequest) async throws -> [String] {
        guard request.style == .topicAcrossLibrary,
              let q = request.freeformQuery, !q.isEmpty
        else { return [] }
        let pages = (try? await wiki.wikiPages(matchingTitle: q)) ?? []
        return pages.prefix(5).map(\.title)
    }

    private func composeViaLLM(
        request: BriefingRequest,
        candidates: [RAGCandidate],
        wikiTitles: [String]
    ) async throws -> LLMScriptDraft {
        let systemPrompt = BriefingPrompts.systemPrompt(for: request.style)
        let userPrompt = BriefingPrompts.userPrompt(
            for: request,
            candidates: candidates,
            wikiTitles: wikiTitles
        )
        let client = WikiOpenRouterClient(
            mode: .live(apiKey: apiKey, modelReference: LLMModelReference(storedID: model))
        )
        let json = try await client.compile(
            systemPrompt: systemPrompt,
            userPrompt: userPrompt,
            feature: CostFeature.briefingCompose
        )
        return try BriefingLLMResponseParser.parse(
            json: json,
            request: request,
            candidates: candidates
        )
    }

    private func synthesizeSegment(
        segment: BriefingSegment,
        candidates: [RAGCandidate],
        segmentsDir: URL
    ) async throws -> [BriefingTrack] {
        // Slice TTS body around quote insertion offsets so each portion of
        // narration becomes its own track. This lets the player surface the
        // currently-playing source (TTS vs. original audio) accurately.
        let sortedQuotes = segment.quotes.sorted { $0.insertAfterChar < $1.insertAfterChar }
        let bodyChars = Array(segment.bodyText)
        var cursor = 0
        var tracks: [BriefingTrack] = []
        var indexInSegment = 0

        for quote in sortedQuotes {
            let pivot = max(cursor, min(quote.insertAfterChar, bodyChars.count))
            let preText = String(bodyChars[cursor..<pivot])
            if !preText.isEmpty {
                let track = try await synthesizeTTSTrack(
                    text: preText, segment: segment,
                    indexInSegment: indexInSegment, segmentsDir: segmentsDir
                )
                tracks.append(track)
                indexInSegment += 1
            }
            tracks.append(makeQuoteTrack(quote: quote, segment: segment, indexInSegment: indexInSegment))
            indexInSegment += 1
            cursor = pivot
        }
        let tail = cursor < bodyChars.count ? String(bodyChars[cursor..<bodyChars.count]) : ""
        if !tail.isEmpty || sortedQuotes.isEmpty {
            let text = tail.isEmpty ? segment.bodyText : tail
            let track = try await synthesizeTTSTrack(
                text: text, segment: segment,
                indexInSegment: indexInSegment, segmentsDir: segmentsDir
            )
            tracks.append(track)
        }
        _ = candidates // Reserved for future re-resolution of attributions.
        return tracks
    }

    private func synthesizeTTSTrack(
        text: String,
        segment: BriefingSegment,
        indexInSegment: Int,
        segmentsDir: URL
    ) async throws -> BriefingTrack {
        let outURL = segmentsDir
            .appendingPathComponent("tts-\(segment.id.uuidString)-\(indexInSegment).m4a")
        let duration = try await tts.synthesize(text: text, voiceID: voiceID, outputURL: outURL)
        return BriefingTrack(
            segmentID: segment.id,
            indexInSegment: indexInSegment,
            kind: .tts,
            audioURL: outURL,
            startInTrackSeconds: 0,
            endInTrackSeconds: duration,
            transcriptText: text,
            attribution: segment.attributions.first
        )
    }

    private func makeQuoteTrack(
        quote: BriefingQuote,
        segment: BriefingSegment,
        indexInSegment: Int
    ) -> BriefingTrack {
        // The stitcher trims `[start, end]` out of the enclosure URL when
        // appending — passing the full URL plus the requested time range is
        // the cleanest way to keep `BriefingTrack` storage-free.
        BriefingTrack(
            segmentID: segment.id,
            indexInSegment: indexInSegment,
            kind: .quote,
            audioURL: quote.enclosureURL,
            startInTrackSeconds: quote.startSeconds,
            endInTrackSeconds: quote.endSeconds,
            transcriptText: quote.transcriptText,
            attribution: BriefingAttribution(
                episodeID: quote.episodeID,
                displayLabel: "Episode · \(formatTime(quote.startSeconds))",
                timestampSeconds: quote.startSeconds
            )
        )
    }

    private func aggregateSources(from segments: [BriefingSegment]) -> [BriefingAttribution] {
        var seen = Set<String>()
        var out: [BriefingAttribution] = []
        for s in segments {
            for a in s.attributions where seen.insert(a.displayLabel).inserted {
                out.append(a)
            }
        }
        return out
    }

    private func formatTime(_ seconds: TimeInterval) -> String {
        let mm = Int(seconds) / 60
        let ss = Int(seconds) % 60
        return String(format: "%d:%02d", mm, ss)
    }
}

enum BriefingComposerError: LocalizedError, Sendable {
    case missingTopic
    case noEvidence(scope: BriefingScope)
    case unsupportedScope(String)

    var errorDescription: String? {
        switch self {
        case .missingTopic:
            return "Add a topic before composing a topic briefing."
        case .noEvidence(let scope):
            return "No transcript evidence was found for \(scope.displayName)."
        case .unsupportedScope(let message):
            return message
        }
    }
}
