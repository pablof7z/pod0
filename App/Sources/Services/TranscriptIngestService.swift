import Foundation
import os.log

// MARK: - TranscriptIngestService
//
// Bounded transcript and semantic-index stages used only by WorkCoordinator.
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
    private let elevenLabsKey: @Sendable () -> String?
    private let openRouterKey: @Sendable () -> String?
    private let assemblyAIKey: @Sendable () -> String?

    // MARK: Init

    init(
        rag: RAGService = .shared,
        ingestor: PublisherTranscriptIngestor = PublisherTranscriptIngestor(),
        scribe: ElevenLabsScribeClient = ElevenLabsScribeClient(),
        whisper: OpenRouterWhisperClient = OpenRouterWhisperClient(),
        assemblyAI: AssemblyAITranscriptClient = AssemblyAITranscriptClient(),
        appleSTT: AppleNativeSTTClient = AppleNativeSTTClient(),
        chunkBuilder: ChunkBuilder = ChunkBuilder(),
        store: TranscriptStore = .shared,
        elevenLabsKey: @escaping @Sendable () -> String? = {
            (try? ElevenLabsCredentialStore.apiKey()).flatMap { $0.isEmpty ? nil : $0 }
        },
        openRouterKey: @escaping @Sendable () -> String? = {
            (try? OpenRouterCredentialStore.apiKey()).flatMap { $0.isEmpty ? nil : $0 }
        },
        assemblyAIKey: @escaping @Sendable () -> String? = {
            (try? AssemblyAICredentialStore.apiKey()).flatMap { $0.isEmpty ? nil : $0 }
        }
    ) {
        self.rag = rag
        self.ingestor = ingestor
        self.scribe = scribe
        self.whisper = whisper
        self.assemblyAI = assemblyAI
        self.appleSTT = appleSTT
        self.chunkBuilder = chunkBuilder
        self.store = store
        self.elevenLabsKey = elevenLabsKey
        self.openRouterKey = openRouterKey
        self.assemblyAIKey = assemblyAIKey
    }

    func resolvedElevenLabsKey() -> String? { elevenLabsKey() }
    func resolvedOpenRouterKey() -> String? { openRouterKey() }
    func resolvedAssemblyAIKey() -> String? { assemblyAIKey() }

    // MARK: - Durable transcript stage

    /// Bounded durable executor entry point. Remote operation identity is
    /// recorded under the active lease before polling, and an existing
    /// AssemblyAI identity is resumed instead of resubmitted.
    func executeJob(
        context: JobAttemptContext,
        payload: TranscriptJobPayload,
        jobStore: JobStore
    ) async throws -> String {
        guard let appStore = rag.appStore,
              let episode = appStore.episode(id: context.job.subjectID) else {
            throw JobFailure(classification: .invalidInput, message: "Episode no longer exists")
        }
        if Self.shouldAttemptPublisher(
            userInitiated: payload.userInitiated,
            externalProvider: context.job.externalProvider,
            externalOperationID: context.job.externalOperationID
        ), let url = episode.publisherTranscriptURL {
            do {
                try jobStore.recordExternalOperation(
                    id: context.job.id,
                    leaseToken: context.leaseToken,
                    provider: "publisherTranscript",
                    externalID: context.job.inputVersion,
                    state: "fetching"
                )
                let transcript = try await ingestor.ingest(
                    url: url,
                    mimeHint: episode.publisherTranscriptType?.rawValue,
                    episodeID: episode.id,
                    language: "en-US"
                )
                return try persistJobTranscript(
                    transcript, context: context
                )
            } catch is CancellationError {
                throw JobFailure(classification: .cancelled, message: "Publisher fetch cancelled")
            } catch {
                Self.logger.notice("Publisher transcript unavailable; falling back: \(error, privacy: .public)")
            }
        }

        let provider = payload.provider
        if provider == .appleNative && !EpisodeDownloadStore.shared.exists(for: episode) {
            throw JobFailure(
                classification: .missingDependency,
                message: "On-device transcription is waiting for downloaded audio."
            )
        }
        let localOrRemote = EpisodeDownloadStore.shared.exists(for: episode)
            ? EpisodeDownloadStore.shared.localFileURL(for: episode)
            : episode.enclosureURL
        let audioURL = provider == .assemblyAI ? episode.enclosureURL : localOrRemote
        let resumedExternalID = try Self.resumableExternalOperationID(
            expectedProvider: Self.externalProviderName(for: provider),
            recordedProvider: context.job.externalProvider,
            recordedID: context.job.externalOperationID
        )
        let transcript: Transcript
        do {
            switch provider {
            case .assemblyAI:
                let models = payload.modelID
                    .split(separator: ",")
                    .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
                    .filter { !$0.isEmpty }
                let selectedModels = models.isEmpty ? ["universal-3-pro", "universal-2"] : models
                let remoteJob: AssemblyAIJob
                if let externalID = resumedExternalID {
                    remoteJob = AssemblyAIJob(
                        transcriptID: externalID,
                        episodeID: episode.id,
                        createdAt: context.job.updatedAt,
                        languageHint: nil,
                        speechModels: selectedModels
                    )
                } else {
                    try jobStore.recordExternalSubmissionIntent(
                        id: context.job.id,
                        leaseToken: context.leaseToken,
                        provider: "assemblyAI"
                    )
                    remoteJob = try await assemblyAI.submit(
                        audioURL: audioURL,
                        episodeID: episode.id,
                        speechModels: selectedModels,
                        speakerLabels: true,
                        languageDetection: true
                    )
                    try jobStore.recordExternalOperation(
                        id: context.job.id,
                        leaseToken: context.leaseToken,
                        provider: "assemblyAI",
                        externalID: remoteJob.transcriptID,
                        state: "submitted"
                    )
                }
                transcript = try await assemblyAI.pollResult(remoteJob)
            case .elevenLabsScribe:
                let remoteJob: ScribeJob
                if let externalID = resumedExternalID {
                    remoteJob = ScribeJob(
                        requestID: externalID,
                        episodeID: episode.id,
                        createdAt: context.job.updatedAt,
                        languageHint: nil,
                        inlineResult: nil
                    )
                } else {
                    try jobStore.recordExternalSubmissionIntent(
                        id: context.job.id,
                        leaseToken: context.leaseToken,
                        provider: "elevenLabsScribe"
                    )
                    remoteJob = try await scribe.submit(
                        audioURL: audioURL,
                        episodeID: episode.id
                    )
                    try jobStore.recordExternalOperation(
                        id: context.job.id,
                        leaseToken: context.leaseToken,
                        provider: "elevenLabsScribe",
                        externalID: remoteJob.requestID,
                        state: "responseReceived"
                    )
                }
                transcript = try await scribe.pollResult(remoteJob)
            case .openRouterWhisper:
                guard resumedExternalID == nil else {
                    throw JobFailure(
                        classification: .unsafeToRetry,
                        message: "OpenRouter transcription cannot resume a recorded external operation."
                    )
                }
                try jobStore.recordExternalSubmissionIntent(
                    id: context.job.id,
                    leaseToken: context.leaseToken,
                    provider: "openRouterWhisper"
                )
                transcript = try await whisper.transcribe(audioURL: audioURL, episodeID: episode.id)
            case .appleNative:
                guard resumedExternalID == nil else {
                    throw JobFailure(
                        classification: .unsafeToRetry,
                        message: "On-device transcription cannot resume a remote operation."
                    )
                }
                transcript = try await appleSTT.transcribe(
                    audioFileURL: audioURL,
                    episodeID: episode.id
                )
            }
        } catch is CancellationError {
            throw JobFailure(classification: .cancelled, message: "Transcription cancelled")
        } catch let failure as JobFailure {
            throw failure
        } catch {
            throw JobFailure(classification: .transient, message: error.localizedDescription)
        }
        return try persistJobTranscript(transcript, context: context)
    }

    /// Once a resumable paid provider identity exists it is authoritative.
    /// Retrying the publisher fallback first could overwrite that identity
    /// and turn a safe poll into a duplicate provider submission after a kill.
    nonisolated static func shouldAttemptPublisher(
        userInitiated: Bool,
        externalProvider: String?,
        externalOperationID: String?
    ) -> Bool {
        guard !userInitiated else { return false }
        return (externalProvider == nil && externalOperationID == nil)
            || externalProvider == "publisherTranscript"
    }

    /// Resolves durable provider evidence without ever laundering a
    /// mismatched or half-recorded submission into a fresh paid request.
    nonisolated static func resumableExternalOperationID(
        expectedProvider: String,
        recordedProvider: String?,
        recordedID: String?
    ) throws -> String? {
        if recordedProvider == nil, recordedID == nil { return nil }
        if recordedProvider == "publisherTranscript" { return nil }
        guard recordedProvider == expectedProvider,
              let recordedID,
              !recordedID.isBlank else {
            throw JobFailure(
                classification: .unsafeToRetry,
                message: "Recorded provider identity does not match the transcript executor."
            )
        }
        return recordedID
    }

    nonisolated private static func externalProviderName(for provider: STTProvider) -> String {
        switch provider {
        case .assemblyAI: "assemblyAI"
        case .elevenLabsScribe: "elevenLabsScribe"
        case .openRouterWhisper: "openRouterWhisper"
        case .appleNative: "appleNative"
        }
    }

    private func persistJobTranscript(
        _ transcript: Transcript,
        context: JobAttemptContext
    ) throws -> String {
        try store.stage(transcript, context: context)
    }

    /// Runs only the vector-index outcome for an already persisted transcript.
    /// Failures escape to `WorkCoordinator`, which owns durable backoff.
    func indexTranscript(
        episodeID: UUID,
        generation: String
    ) async throws -> VectorArtifactReceipt {
        guard let appStore = rag.appStore,
              let episode = appStore.episode(id: episodeID) else {
            throw JobFailure(classification: .invalidInput, message: "Episode no longer exists")
        }
        guard case .ready = episode.transcriptState,
              let transcript = store.load(episodeID: episodeID) else {
            throw JobFailure(
                classification: .missingDependency,
                message: "Transcript is not available for indexing."
            )
        }
        let chunkable = ChunkableTranscript(
            transcript: transcript,
            podcastID: episode.podcastID
        )
        let chunks = chunkBuilder.build(from: chunkable)
        let receipt = try await rag.index.stageArtifact(
            chunks: chunks,
            episodeID: episode.id,
            generation: generation,
            artifactKind: VectorIndex.semanticArtifactKind
        )
        Self.logger.info(
            "indexed \(chunks.count, privacy: .public) transcript chunks for \(episode.id, privacy: .public)"
        )
        return receipt
    }

    // MARK: - Helpers

    static func isReady(_ state: TranscriptState) -> Bool {
        if case .ready = state { return true }
        return false
    }
}
