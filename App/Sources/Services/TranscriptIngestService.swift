import Foundation
import os.log

// MARK: - TranscriptIngestService
//
// Executes the host-side transcript ingestion pipeline from a Rust-owned plan:
//   1. Ask Rust whether to skip, fetch publisher transcript, or run STT.
//   2. Execute the returned native/network capability branch.
//   3. Persist the parsed `Transcript` JSON to disk for the EpisodeDetail view.
//   4. Report the result/status back to the kernel.
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

    private let ingestor: PublisherTranscriptIngestor
    private let scribe: ElevenLabsScribeClient
    private let whisper: OpenRouterWhisperClient
    private let assemblyAI: AssemblyAITranscriptClient
    private let appleSTT: AppleNativeSTTClient
    private let store: TranscriptStore

    // MARK: AppStateStore handle

    /// Weak reference to the live store. Set once by `AppStateStore.init`
    /// via `attach(appStore:)`. The service is a singleton; `AppStateStore`
    /// keeps no back-reference so there is no retain cycle.
    weak var appStore: AppStateStore?

    func attach(appStore: AppStateStore) {
        self.appStore = appStore
    }

    // MARK: In-flight tracking (dedup)

    private var inFlight: Set<UUID> = []

    // MARK: Init

    init(
        ingestor: PublisherTranscriptIngestor = PublisherTranscriptIngestor(),
        scribe: ElevenLabsScribeClient = ElevenLabsScribeClient(),
        whisper: OpenRouterWhisperClient = OpenRouterWhisperClient(),
        assemblyAI: AssemblyAITranscriptClient = AssemblyAITranscriptClient(),
        appleSTT: AppleNativeSTTClient = AppleNativeSTTClient(),
        store: TranscriptStore = .shared
    ) {
        self.ingestor = ingestor
        self.scribe = scribe
        self.whisper = whisper
        self.assemblyAI = assemblyAI
        self.appleSTT = appleSTT
        self.store = store
    }

    // MARK: Public API

    /// Ingest the transcript for one episode. Resolves the publisher URL +
    /// type from `AppStateStore`, fetches, parses, chunks, embeds, upserts,
    /// then persists the parsed transcript to disk and updates state.
    /// Idempotent — repeat calls for the same episode no-op while a prior
    /// call is in flight.
    ///
    /// - Parameter forceProvider: When non-nil, asks Rust to plan an explicit
    ///   one-off STT retry with this provider instead of the global provider.
    ///   Used by the Diagnostics "Retry with…" menu so the user can try an
    ///   alternative provider for one call without flipping their global
    ///   setting. `nil` preserves Rust's publisher-first behaviour.
    func ingest(episodeID: UUID, forceProvider: STTProvider? = nil) async {
        guard let appStore = appStore else {
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
        inFlight.insert(episodeID)
        defer { inFlight.remove(episodeID) }

        let localAudioAvailable = EpisodeDownloadStore.shared.exists(for: episode)
        guard let plan = appStore.kernel?.transcriptIngestPlan(
            episodeID: episodeID,
            forceProvider: forceProvider,
            localAudioAvailable: localAudioAvailable,
            allowPublisher: forceProvider == nil
        ) else {
            Self.logger.warning("ingest(\(episodeID, privacy: .public)): Rust plan unavailable")
            return
        }
        switch plan.status {
        case "ready":
            return
        case "skipped":
            appStore.kernelRecordTranscriptSkip(
                episodeID: episodeID,
                reason: plan.reason ?? "Transcription skipped."
            )
            appStore.setEpisodeTranscriptState(episodeID, state: .none)
            return
        case "stt":
            await executeSTTPlan(plan, episode: episode, appStore: appStore, explicit: forceProvider != nil)
            return
        case "publisher":
            guard let urlString = plan.publisherUrl,
                  let url = URL(string: urlString) else {
                appStore.setEpisodeTranscriptState(
                    episodeID,
                    state: .failed(message: "Publisher transcript URL was invalid.")
                )
                return
            }
            appStore.setEpisodeTranscriptState(episodeID, state: .fetchingPublisher)
            let fetched: Transcript?
            do {
                fetched = try await ingestor.ingest(
                    url: url,
                    mimeHint: plan.mimeHint,
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
            // fetched == nil: ask Rust for the next plan with publisher disabled.
            guard let fallbackPlan = appStore.kernel?.transcriptIngestPlan(
                episodeID: episodeID,
                forceProvider: nil,
                localAudioAvailable: localAudioAvailable,
                allowPublisher: false
            ) else { return }
            if fallbackPlan.status == "stt" {
                await executeSTTPlan(fallbackPlan, episode: episode, appStore: appStore, explicit: false)
            } else if fallbackPlan.status == "skipped" {
                appStore.kernelRecordTranscriptSkip(
                    episodeID: episodeID,
                    reason: fallbackPlan.reason ?? "Transcription skipped."
                )
                appStore.setEpisodeTranscriptState(episodeID, state: .none)
            } else if fallbackPlan.status == "ready" {
                return
            } else {
                appStore.setEpisodeTranscriptState(
                    episodeID,
                    state: .failed(message: fallbackPlan.reason ?? "Transcription could not start.")
                )
            }
            return
        default:
            appStore.setEpisodeTranscriptState(
                episodeID,
                state: .failed(message: plan.reason ?? "Transcription could not start.")
            )
            return
        }
    }

    // MARK: - Private pipeline

    private func executeSTTPlan(
        _ plan: KernelModel.TranscriptIngestPlan,
        episode: Episode,
        appStore: AppStateStore,
        explicit: Bool
    ) async {
        guard let raw = plan.provider,
              let provider = STTProvider(rawValue: raw) else {
            appStore.setEpisodeTranscriptState(
                episode.id,
                state: .failed(message: plan.reason ?? "Transcription provider was unavailable.")
            )
            return
        }
        await runAITranscription(
            for: episode,
            provider: provider,
            appStore: appStore,
            explicit: explicit
        )
    }

    private func runAITranscription(
        for episode: Episode,
        provider: STTProvider,
        appStore: AppStateStore,
        explicit: Bool
    ) async {
        // Defensive race guard only: Rust's transcript plan owns the
        // Apple-native local-file policy. If the file disappears between
        // planning and execution, avoid invoking the native recognizer with a
        // dead URL.
        if provider == .appleNative && !EpisodeDownloadStore.shared.exists(for: episode) {
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
            provider: Self.providerDisplayName(provider, kernel: appStore.kernel) ?? provider.rawValue
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
            source: Self.sourceDisplayName(source, kernel: appStore.kernel)
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

        // STEP 2: Fire-and-forget AI chapter compilation via the kernel (D0).
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

}
