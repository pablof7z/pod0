import Foundation

// MARK: - TranscriptIngestService auto-ingest decision logic
//
// Split out from TranscriptIngestService.swift so the main file stays under
// AGENTS.md's 500-line hard cap. These methods don't touch the per-episode
// `inFlight`/`ingest` machinery - they only decide *which* episodes to push
// into `ingest()`. The unit tests in `TranscriptAutoIngestTests` lean on the
// `autoIngestCandidates` pure-function entry point.

extension TranscriptIngestService {

    /// Convenience: walk the store and ingest up to `maxCount` episodes that
    /// are not yet `.ready` and have a publisher transcript URL. Useful as a
    /// background warmup once the user lands on the library.
    func ingestPending(maxCount: Int = 5) async {
        guard let appStore = rag.appStore else {
            return
        }
        let candidates = appStore.state.episodes
            .filter { $0.publisherTranscriptURL != nil && !Self.isReady($0.transcriptState) }
            .sorted { $0.pubDate > $1.pubDate }
            .prefix(maxCount)
        for episode in candidates {
            await ingest(episodeID: episode.id)
        }
    }

    /// Triggered from `AppStateStore.upsertEpisodes` whenever a feed refresh
    /// surfaces brand-new episode IDs. Filters to episodes the user would
    /// benefit from ingesting and dispatches an async `ingest` per candidate.
    ///
    /// **Inclusion rule** (the unlock for cross-episode RAG):
    ///   - Episode is not already `.ready`.
    ///   - At least one path is available - either a `publisherTranscriptURL`
    ///     with `autoIngestPublisherTranscripts` on, OR a configured
    ///     ElevenLabs key with `autoFallbackToScribe` on.
    ///
    /// `ingest()` itself handles per-category opt-out, dedup, and the
    /// publisher -> Scribe fallback inside one call - so this method only has
    /// to decide *whether to bother trying*. Without the relaxed filter,
    /// shows that don't ship `<podcast:transcript>` (most indie podcasts)
    /// get NOTHING auto-fetched even with Scribe configured, and the agent's
    /// RAG layer comes up dark for those subscriptions.
    func evaluateAutoIngest(newEpisodeIDs: [UUID]) {
        guard !newEpisodeIDs.isEmpty else { return }
        guard let appStore = rag.appStore else { return }
        let episodes = newEpisodeIDs.compactMap { appStore.episode(id: $0) }
        let candidates = Self.autoIngestCandidates(
            among: episodes,
            settings: appStore.state.settings,
            elevenLabsKey: resolvedElevenLabsKey(),
            openRouterKey: resolvedOpenRouterKey(),
            assemblyAIKey: resolvedAssemblyAIKey()
        )
        guard !candidates.isEmpty else { return }
        for episodeID in candidates {
            Task { @MainActor [weak self] in
                await self?.ingest(episodeID: episodeID)
            }
        }
    }

    /// Pure decision logic for `evaluateAutoIngest`. Exposed `internal` so
    /// `TranscriptAutoIngestTests` can pin the branching without driving the
    /// full ingest pipeline (which needs network + ElevenLabs + sqlite-vec).
    ///
    /// Inclusion rule:
    ///   - Episode is not already `.ready`.
    ///   - At least one path is available - either the publisher transcript
    ///     URL is present and `autoIngestPublisherTranscripts` is on, OR the
    ///     ElevenLabs key is configured and `autoFallbackToScribe` is on.
    static func autoIngestCandidates(
        among episodes: [Episode],
        settings: Settings,
        elevenLabsKey: String?,
        openRouterKey: String? = nil,
        assemblyAIKey: String? = nil
    ) -> [UUID] {
        let publisherOn = settings.autoIngestPublisherTranscripts
        let sttReady: Bool
        switch settings.sttProvider {
        case .appleNative: sttReady = true   // no API key needed
        case .openRouterWhisper: sttReady = !(openRouterKey ?? "").isEmpty
        case .assemblyAI: sttReady = !(assemblyAIKey ?? "").isEmpty
        case .elevenLabsScribe: sttReady = !(elevenLabsKey ?? "").isEmpty
        }
        let scribeOn = settings.autoFallbackToScribe && sttReady
        guard publisherOn || scribeOn else { return [] }
        return episodes.compactMap { episode -> UUID? in
            guard !Self.isReady(episode.transcriptState) else { return nil }
            if episode.publisherTranscriptURL != nil {
                return publisherOn ? episode.id : nil
            }
            return scribeOn ? episode.id : nil
        }
    }
}
