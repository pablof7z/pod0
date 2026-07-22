import Foundation
import Pod0Core
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

    nonisolated static let logger = Logger.app("TranscriptIngestService")

    // MARK: Dependencies

    private(set) weak var appStore: AppStateStore?
    private let ingestor: PublisherTranscriptIngestor
    private let scribe: ElevenLabsScribeClient
    private let whisper: OpenRouterWhisperClient
    private let assemblyAI: AssemblyAITranscriptClient
    private let appleSTT: AppleNativeSTTClient
    private let elevenLabsKey: @Sendable () -> String?
    private let openRouterKey: @Sendable () -> String?
    private let assemblyAIKey: @Sendable () -> String?

    // MARK: Init

    init(
        ingestor: PublisherTranscriptIngestor = PublisherTranscriptIngestor(),
        scribe: ElevenLabsScribeClient = ElevenLabsScribeClient(),
        whisper: OpenRouterWhisperClient = OpenRouterWhisperClient(),
        assemblyAI: AssemblyAITranscriptClient = AssemblyAITranscriptClient(),
        appleSTT: AppleNativeSTTClient = AppleNativeSTTClient(),
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
        self.ingestor = ingestor
        self.scribe = scribe
        self.whisper = whisper
        self.assemblyAI = assemblyAI
        self.appleSTT = appleSTT
        self.elevenLabsKey = elevenLabsKey
        self.openRouterKey = openRouterKey
        self.assemblyAIKey = assemblyAIKey
    }

    func attach(appStore: AppStateStore) {
        self.appStore = appStore
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
        guard let appStore,
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
                return try await persistJobTranscript(
                    transcript, context: context
                )
            } catch is CancellationError {
                throw JobFailure(classification: .cancelled, message: "Publisher fetch cancelled")
            } catch {
                let failure = ProductFailure.classify(error)
                Self.logger.notice(
                    "Publisher transcript unavailable; falling back: \(failure.code.rawValue, privacy: .public)"
                )
            }
        }

        let provider = payload.provider
        let localAudio = episode.downloadState.localFileURL.flatMap { url in
            FileManager.default.fileExists(atPath: url.path) ? url : nil
        }
        if provider == .appleNative && localAudio == nil {
            throw JobFailure(
                classification: .missingDependency,
                message: "On-device transcription is waiting for downloaded audio."
            )
        }
        let localOrRemote = localAudio ?? episode.enclosureURL
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
        } catch { throw JobFailure.classify(error) }
        return try await persistJobTranscript(transcript, context: context)
    }

    private func persistJobTranscript(
        _ transcript: Transcript,
        context: JobAttemptContext
    ) async throws -> String {
        guard let episode = appStore?.episode(id: transcript.episodeID),
              let sharedLibrary = appStore?.sharedLibrary else {
            throw JobFailure(
                classification: .missingDependency,
                message: "Shared transcript core is unavailable."
            )
        }
        let payload = try Self.transcriptEncoder.encode(transcript)
        let result = try await sharedLibrary.submitTranscriptObservationOffMain(
            transcript,
            context: TranscriptObservationContext(
                podcastID: episode.podcastID,
                sourceRevision: context.job.inputVersion,
                sourcePayloadDigest: ArtifactRepository.hash(payload),
                provider: TranscriptObservationMapper.defaultProvider(for: transcript.source)
            ),
            commandID: CommandId(uuid: context.job.id),
            cancellationID: CancellationId(uuid: context.leaseToken)
        )
        let receipt = try SharedTranscriptWorkflowReceipt(
            summary: result.summary,
            inputVersion: context.job.inputVersion
        )
        return try Self.transcriptEncoder.encode(receipt).base64EncodedString()
    }

    // MARK: - Helpers

    static func isReady(_ state: TranscriptState) -> Bool {
        if case .ready = state { return true }
        return false
    }

    private static let transcriptEncoder: JSONEncoder = {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.sortedKeys]
        return encoder
    }()
}
