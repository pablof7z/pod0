import Foundation
import os.log

// MARK: - TranscriptIngestService
//
// Owns the end-to-end transcript ingestion pipeline:
//   1. Pick a publisher transcript URL (or fall back to the selected STT provider).
//   2. Fetch + parse via `PublisherTranscriptIngestor`.
//   3. Slice into `Chunk`s via `ChunkBuilder`, supplying podcast/episode FKs.
//   4. Embed + upsert via `RAGService.shared.index` (which calls the embedder).
//   5. Persist the parsed `Transcript` JSON to disk for the EpisodeDetail view.
//   6. Update `Episode.transcriptState` on the live `AppStateStore`.
//
// The service stays `@MainActor` because every input + output it touches
// (state store, episode model, status flips) lives on the main actor; the
// expensive bits (network, SQLite, embedding) all hop off via `await`.

@MainActor
final class TranscriptIngestService {

    // MARK: Singleton

    static let shared = TranscriptIngestService()

    // MARK: Logger

    nonisolated private static let logger = Logger.app("TranscriptIngestService")

    // MARK: Dependencies

    let rag: RAGService
    private let ingestor: PublisherTranscriptIngestor
    private let scribe: ElevenLabsScribeClient
    private let whisper: OpenRouterWhisperClient
    private let assemblyAI: AssemblyAITranscriptClient
    private let appleSTT: AppleNativeSTTClient
    private let chunkBuilder: ChunkBuilder
    private let store: TranscriptStore

    // MARK: In-flight tracking (dedup)

    private var inFlight: Set<UUID> = []

    // MARK: Init

    init(
        rag: RAGService = .shared,
        ingestor: PublisherTranscriptIngestor = PublisherTranscriptIngestor(),
        scribe: ElevenLabsScribeClient = ElevenLabsScribeClient(),
        whisper: OpenRouterWhisperClient = OpenRouterWhisperClient(),
        assemblyAI: AssemblyAITranscriptClient = AssemblyAITranscriptClient(),
        appleSTT: AppleNativeSTTClient = AppleNativeSTTClient(),
        chunkBuilder: ChunkBuilder = ChunkBuilder(),
        store: TranscriptStore = .shared
    ) {
        self.rag = rag
        self.ingestor = ingestor
        self.scribe = scribe
        self.whisper = whisper
        self.assemblyAI = assemblyAI
        self.appleSTT = appleSTT
        self.chunkBuilder = chunkBuilder
        self.store = store
    }

    // MARK: Public API

    /// Ingest the transcript for one episode. Resolves the publisher URL +
    /// type from `AppStateStore`, fetches, parses, chunks, embeds, upserts,
    /// then persists the parsed transcript to disk and updates state.
    /// Idempotent — repeat calls for the same episode no-op while a prior
    /// call is in flight.
    ///
    /// - Parameter forceProvider: When non-nil, bypass the publisher fetch
    ///   path and the `autoFallbackToScribe` gate, and use this provider
    ///   instead of `settings.sttProvider` for AI transcription. Used by the
    ///   Diagnostics "Retry with…" menu so the user can try an alternative
    ///   provider for one call without flipping their global setting. `nil`
    ///   preserves existing publisher-first behaviour.
    func ingest(episodeID: UUID, forceProvider: STTProvider? = nil) async {
        guard let appStore = rag.appStore else {
            Self.logger.warning(
                "ingest(\(episodeID, privacy: .public)): no AppStateStore attached — skipping"
            )
            return
        }
        guard !inFlight.contains(episodeID) else {
            Self.logger.debug(
                "ingest(\(episodeID, privacy: .public)): already in flight — skipping"
            )
            return
        }
        guard let episode = appStore.episode(id: episodeID) else {
            Self.logger.warning(
                "ingest(\(episodeID, privacy: .public)): episode not found in store"
            )
            return
        }
        // Skip episodes that already have a transcript unless the caller forced
        // a specific provider (an explicit Diagnostics "Retry with…", which
        // resets state to `.none` first). This makes `ingest()` safe to fire
        // unconditionally from the post-download hook and every other auto
        // caller without re-running a transcription that already succeeded.
        if forceProvider == nil, Self.isReady(episode.transcriptState) {
            Self.logger.debug(
                "ingest(\(episodeID, privacy: .public)): already has a transcript — skipping"
            )
            return
        }
        // Per-category opt-out: if the user has disabled transcription for
        // the category this show belongs to, skip ingestion entirely.
        // Defaults to allow when the show isn't yet categorised.
        guard appStore.effectiveTranscriptionEnabled(forPodcast: episode.podcastID) else {
            Self.logger.info(
                "ingest(\(episodeID, privacy: .public)): transcription disabled for category — skipping"
            )
            appStore.kernelRecordTranscriptSkip(
                episodeID: episodeID,
                reason: "Transcription is turned off for this show's category."
            )
            return
        }

        inFlight.insert(episodeID)
        defer { inFlight.remove(episodeID) }

        // Force-provider path: the user picked a specific provider in
        // Diagnostics. Skip the publisher fetch and the autoFallback gate
        // and go straight to the chosen STT provider.
        if let forced = forceProvider {
            // The Diagnostics "Retry with…" override is an explicit user pick,
            // NOT the fallback policy — so it bypasses the kernel-resolved
            // provider and runs exactly what was chosen. Shared-backend-owned
            // providers report missing-key/provider errors from Rust.
            guard forcedProviderHasKey(forced) else {
                Self.logger.info(
                    "forceProvider=\(forced.displayName, privacy: .public) but no key configured — leaving transcriptState=.none"
                )
                appStore.kernelRecordTranscriptSkip(
                    episodeID: episodeID,
                    reason: "No API key is configured for \(forced.displayName)."
                )
                appStore.setEpisodeTranscriptState(episodeID, state: .none)
                return
            }
            await runAITranscription(
                for: episode, provider: forced, appStore: appStore, explicit: true
            )
            return
        }

        // Path A: publisher transcript URL.
        //
        // Two error stages here, kept distinct so the log reflects which
        // step actually failed. The fetch+parse stage tells us whether
        // the publisher URL was usable at all; the persist stage tells
        // us whether on-disk storage worked. With the persistAndIndex
        // refactor, embedding failures no longer throw — so a thrown
        // error from `persistAndIndex` is a real disk problem, not a
        // missing-key one, and falling through to Scribe wouldn't help.
        if let url = episode.publisherTranscriptURL {
            appStore.setEpisodeTranscriptState(episodeID, state: .fetchingPublisher)
            let fetched: Transcript?
            do {
                fetched = try await ingestor.ingest(
                    url: url,
                    mimeHint: episode.publisherTranscriptType?.rawValue,
                    episodeID: episodeID,
                    language: "en-US"
                )
            } catch {
                Self.logger.notice(
                    "publisher transcript fetch failed for \(episodeID, privacy: .public): \(String(describing: error), privacy: .public) — trying Scribe fallback"
                )
                // Reset state so the Scribe path can take over cleanly.
                appStore.setEpisodeTranscriptState(episodeID, state: .none)
                fetched = nil
            }
            if let transcript = fetched {
                do {
                    try await persistAndIndex(
                        transcript: transcript,
                        episode: episode,
                        source: .publisher,
                        appStore: appStore
                    )
                    return
                } catch {
                    Self.logger.error(
                        "publisher transcript persist failed for \(episodeID, privacy: .public): \(String(describing: error), privacy: .public) — disk error, not falling through to Scribe"
                    )
                    appStore.setEpisodeTranscriptState(
                        episodeID,
                        state: .failed(message: error.localizedDescription)
                    )
                    return
                }
            }
            // fetched == nil: fetch threw above; let the Scribe path below run.
        }

        // Path B: AI transcription fallback (ElevenLabs Scribe or OpenRouter Whisper).
        guard appStore.state.settings.autoFallbackToScribe else {
            Self.logger.info(
                "publisher transcript missing for \(episodeID, privacy: .public) and AI transcription disabled in settings — leaving transcriptState=.none"
            )
            appStore.kernelRecordTranscriptSkip(
                episodeID: episodeID,
                reason: "No publisher transcript, and automatic AI transcription is off (turn it on in Settings)."
            )
            appStore.setEpisodeTranscriptState(episodeID, state: .none)
            return
        }
        // The STT provider fallback policy is kernel-owned. Read the resolved
        // provider the kernel computed (selection + key-presence) rather than
        // re-deriving it in Swift. `effectiveSttProvider` already downgrades a
        // key-requiring provider whose key is absent to `apple_native`; an
        // empty/missing snapshot defaults to `.appleNative` (always available).
        let resolvedRaw = appStore.kernel?.podcastSnapshot?.settings.effectiveSttProvider
            ?? STTProvider.appleNative.rawValue
        let provider = STTProvider(rawValue: resolvedRaw) ?? .appleNative
        await runAITranscription(
            for: episode, provider: provider, appStore: appStore, explicit: false
        )
    }

    // MARK: - Private pipeline

    private func runAITranscription(
        for episode: Episode,
        provider: STTProvider,
        appStore: AppStateStore,
        explicit: Bool
    ) async {
        // Apple on-device STT requires a local file. Skip silently rather than
        // setting `.failed` — the post-download hook in `DownloadCapability`
        // re-enters `ingest()` once the file lands, at which point this guard
        // passes and the AI run proceeds. Setting `.failed` here at
        // feed-refresh time would mark every Apple-Native-bound episode as
        // failed before the user has
        // done anything, which is misleading.
        if provider == .appleNative && !EpisodeDownloadStore.shared.exists(for: episode) {
            // A speculative (auto-ingest) run stays silent so undownloaded
            // episodes don't flood the log — the post-download re-entry handles
            // them. But when the user *explicitly* picked on-device transcription
            // from the Diagnostics "Retry with…" menu, a missing file is a dead
            // end they asked about, so surface it in the event log.
            if explicit {
                appStore.kernelRecordTranscriptSkip(
                    episodeID: episode.id,
                    reason: "On-device transcription needs the episode downloaded, but the audio file wasn't found."
                )
            }
            return
        }
        // Name the provider on the transition so the kernel's
        // `transcript.attempt` Diagnostics event reads "Transcribing audio ·
        // ElevenLabs Scribe", not a bare stage.
        appStore.setEpisodeTranscriptState(
            episode.id,
            state: .transcribing(progress: 0),
            provider: provider.displayName
        )
        // Prefer the on-disk download when present. Provider-specific upload
        // and remote-source handling lives behind each provider client. The
        // kernel's status-path already authors the provider-named
        // `transcript.attempt` event (via the `provider:` we passed above), so
        // we don't duplicate it here.
        let audioURL: URL
        if EpisodeDownloadStore.shared.exists(for: episode) {
            audioURL = EpisodeDownloadStore.shared.localFileURL(for: episode)
        } else {
            audioURL = episode.enclosureURL
        }
        // AssemblyAI is URL-based and fetches the audio server-side. Even when
        // we have the file on disk, we override `audioURL` to the publisher
        // enclosure so we don't try to base64-encode a 90+ MB local file
        // through the gateway (the client rejects file:// URLs anyway).
        let effectiveAudioURL: URL = (provider == .assemblyAI) ? episode.enclosureURL : audioURL
        do {
            let transcript: Transcript
            switch provider {
            case .elevenLabsScribe:
                appStore.kernelSetProviderApiKeys()
                let job = try await scribe.submit(audioURL: effectiveAudioURL, episodeID: episode.id)
                transcript = try await scribe.pollResult(job)
            case .assemblyAI:
                appStore.kernelSetProviderApiKeys()
                let job = try await assemblyAI.submit(audioURL: effectiveAudioURL, episodeID: episode.id)
                transcript = try await assemblyAI.pollResult(job)
            case .openRouterWhisper:
                appStore.kernelSetProviderApiKeys()
                transcript = try await whisper.transcribe(audioURL: effectiveAudioURL, episodeID: episode.id)
            case .appleNative:
                transcript = try await appleSTT.transcribe(audioFileURL: effectiveAudioURL, episodeID: episode.id)
            }
            let stateSource: TranscriptState.Source
            switch provider {
            case .elevenLabsScribe: stateSource = .scribe
            case .assemblyAI: stateSource = .assemblyAI
            case .openRouterWhisper: stateSource = .whisper
            case .appleNative: stateSource = .onDevice
            }
            try await persistAndIndex(
                transcript: transcript,
                episode: episode,
                source: stateSource,
                appStore: appStore
            )
        } catch {
            Self.logger.error(
                "AI transcription failed for \(episode.id, privacy: .public): \(String(describing: error), privacy: .public)"
            )
            appStore.setEpisodeTranscriptState(
                episode.id,
                state: .failed(message: error.localizedDescription)
            )
        }
    }

    /// Whether the force-chosen STT provider has a usable Keychain key.
    ///
    /// Only used by the Diagnostics "Retry with…" override path. The general
    /// fallback policy (which provider to use when a key is missing) is
    /// kernel-owned — see `effective_stt_provider` in the Rust kernel and the
    /// `settings.effectiveSttProvider` snapshot field. `.appleNative` is
    /// keyless and always available. Shared STT transports also return `true`
    /// here because Rust owns their credential error reporting.
    private func forcedProviderHasKey(_ provider: STTProvider) -> Bool {
        switch provider {
        case .elevenLabsScribe: return true
        case .openRouterWhisper: return true
        case .assemblyAI: return true
        case .appleNative: return true
        }
    }

    private func persistAndIndex(
        transcript: Transcript,
        episode: Episode,
        source: TranscriptState.Source,
        appStore: AppStateStore
    ) async throws {
        // STEP 1: Persist + flip to `.ready` BEFORE embedding.
        //
        // The user's primary value out of this pipeline is "I can read the
        // transcript." Embedding for RAG search is a nice-to-have that
        // requires a separate provider key (`OpenRouter` / `Ollama`).
        // Folding embedding into the same throw-path used to mean: a fresh
        // user with no embeddings key never sees a transcript at all,
        // because `VectorIndex.upsert` throws `.missingAPIKey` and the
        // caller catches it and either falls through to Scribe (which
        // would hit the same throw) or sets `.failed`. This was the
        // default first-run experience and matches the reported bug:
        // "no transcript works, not even ElevenLabs Scribe."
        //
        // Save first, mark ready, then attempt embedding. RAG just won't
        // find this episode's content until the user adds an embeddings
        // key and explicitly re-embeds; the transcript itself is readable
        // immediately.
        try store.save(transcript)
        appStore.setEpisodeTranscriptState(
            episode.id,
            state: .ready(source: source)
        )
        // M5.2 / slice 5a: report the transcript to the Rust kernel so AI
        // features (wiki, chapters, RAG context, agent chat) can access it
        // without going through Swift's TranscriptStore. D7: the iOS
        // capability performs the work and reports the result; Rust decides
        // what to do with it.
        //
        // Slice 5a: pass the full Transcript (with timed segments) so the
        // kernel can produce RAG chunks with real start_secs/end_secs. The
        // kernel derives plain text from the entries itself.  Name the
        // producing service so the `transcript.ready` Diagnostics event reads
        // "Transcript ready · ElevenLabs Scribe".
        appStore.kernelTranscriptReport(
            episodeID: episode.id,
            transcript: transcript,
            source: Self.sourceDisplayName(source)
        )
        // Slice 5c: index this episode in the kernel KnowledgeStore so kernel
        // Search (the iOS Search tab) can find newly-transcribed content without
        // waiting for a re-launch.  Ordering: `kernelTranscriptReport` above
        // stores the timed segments on the Rust actor FIRST; `index_episode`
        // chunks the stored text synchronously and embeds off-actor.
        // Fire-and-forget + idempotent (deletes + re-upserts).  Live STT
        // completes one episode at a time, so the single per-episode bump cost
        // (actor chunk pass + main-thread snapshot decode + embed spawn) is fine
        // with no pacing needed — the batch-pacing in the launch-time backfill
        // migration (slice 4) is only necessary because that path dispatches N
        // episodes in a tight loop at cold start.
        appStore.kernelIndexEpisodeKnowledge(episodeID: episode.id)

        // STEP 2: Best-effort embed. Failures are logged but don't throw.
        let chunkable = ChunkableTranscript(
            transcript: transcript,
            podcastID: episode.podcastID
        )
        let chunks = chunkBuilder.build(from: chunkable)

        do {
            // Drop any prior chunks for this episode so re-ingestion
            // replaces rather than accumulates. Idempotent on chunk.id,
            // but old chunks from a different segment-boundary run would
            // otherwise linger. This also wipes any synthetic
            // title/description chunk produced by `EpisodeMetadataIndexer`
            // — transcript chunks subsume that signal.
            try await rag.index.deleteAll(forEpisodeID: episode.id)
            if !chunks.isEmpty {
                try await rag.index.upsert(chunks: chunks)
                // Transcript chunks cover this episode in the RAG index,
                // so the metadata-indexer backfill should skip it next
                // launch.
                appStore.setEpisodesMetadataIndexed([episode.id])
            }
            Self.logger.info(
                "ingested transcript for \(episode.id, privacy: .public) — \(chunks.count, privacy: .public) chunks indexed, source=\(String(describing: source), privacy: .public)"
            )
            // Surface RAG indexing in Diagnostics — the transcript is readable
            // before this, but whether search can find it is invisible
            // otherwise. Empty chunks ⇒ nothing to index (still informative).
            appStore.kernelRecordEpisodeEvent(
                episodeID: episode.id,
                kind: "transcript.indexed",
                severity: "success",
                summary: chunks.isEmpty
                    ? "No transcript chunks to index"
                    : "Indexed for search · \(chunks.count) chunks",
                details: [("Chunks", String(chunks.count))]
            )
        } catch {
            Self.logger.notice(
                "transcript saved for \(episode.id, privacy: .public) but RAG indexing failed: \(String(describing: error), privacy: .public) — episode is readable; search won't find it until the user re-embeds with a configured key"
            )
            // A failed embed isn't fatal (the transcript reads fine), but the
            // user should be able to see *why* search can't find this episode.
            appStore.kernelRecordEpisodeEvent(
                episodeID: episode.id,
                kind: "transcript.index.failed",
                severity: "warning",
                summary: "Search indexing failed — transcript still readable",
                details: [("Error", (error as? LocalizedError)?.errorDescription
                    ?? error.localizedDescription)]
            )
        }

        // STEP 3: Fire-and-forget AI chapter compilation via the kernel (D0).
        // The kernel runs FULL or ENRICH-ONLY mode depending on whether publisher
        // chapters already exist and gates on whether ad detection has already run.
        // Decoupled from the embed step: chapter compilation runs even when
        // embeddings can't (no API key).
        let episodeID = episode.id
        Task { @MainActor [weak appStore] in
            guard let appStore else { return }
            appStore.kernelCompileChapters(episodeID: episodeID)
        }

    }

    // MARK: - Helpers

    static func isReady(_ state: TranscriptState) -> Bool {
        if case .ready = state { return true }
        return false
    }
}
