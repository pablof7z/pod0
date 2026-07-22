import Foundation
import Pod0Core

actor LiveCoreTranscriptTransport: CoreTranscriptTransporting {
    let session: URLSession
    private let providers: CoreTranscriptProviderClients

    init(
        session: URLSession = .shared,
        providers: CoreTranscriptProviderClients? = nil
    ) {
        self.session = session
        self.providers = providers ?? .live(session: session)
    }

    func execute(
        _ request: TranscriptCapabilityRequest
    ) async throws -> CoreTranscriptTransportObservation {
        switch request {
        case .fetchPublisher(let context, let sourceURL, let mimeHint, let maximumBytes):
            return try await fetchPublisher(
                context: context,
                sourceURL: sourceURL,
                mimeHint: mimeHint,
                maximumResponseBytes: maximumBytes
            )
        case .submitProvider(
            let context,
            _,
            _,
            let provider,
            let model,
            let audioURL,
            let maximumBytes
        ):
            return try await submit(
                context: context,
                provider: provider,
                model: model,
                audioURL: audioURL,
                maximumResponseBytes: maximumBytes
            )
        case .recoverProvider(
            let context,
            _,
            _,
            let provider,
            let model,
            let externalID,
            _,
            let maximumBytes
        ):
            return try await recover(
                context: context,
                provider: provider,
                model: model,
                externalOperationID: externalID,
                maximumResponseBytes: maximumBytes
            )
        case .transcribeLocal(let context, _, let audioURL, let locale):
            guard let url = URL(string: audioURL), let episodeID = context.episodeId.uuid else {
                throw CoreTranscriptTransportError.invalidRequest
            }
            let transcript = try await providers.appleSpeech.transcribe(
                audioFileURL: url,
                episodeID: episodeID,
                languageHint: locale
            )
            return .completed(
                transcript: transcript,
                externalOperationID: nil,
                status: "completed"
            )
        }
    }

    private func submit(
        context: TranscriptCapabilityContext,
        provider: TranscriptProvider,
        model: String,
        audioURL: String,
        maximumResponseBytes: UInt64
    ) async throws -> CoreTranscriptTransportObservation {
        guard let url = URL(string: audioURL), let episodeID = context.episodeId.uuid else {
            throw CoreTranscriptTransportError.invalidRequest
        }
        switch provider {
        case .assemblyAi:
            let job = try await providers.assemblyAI.submit(
                audioURL: url,
                episodeID: episodeID,
                speechModels: [model],
                speakerLabels: true,
                languageDetection: true,
                languageHint: nil
            )
            return .providerAccepted(externalOperationID: job.transcriptID, status: "queued")
        case .elevenLabsScribe:
            let client = providers.elevenLabs(model)
            let job = try await client.submit(
                audioURL: url,
                episodeID: episodeID,
                languageHint: nil
            )
            let transcript = try await client.result(for: job)
            try enforceBound(transcript, maximumResponseBytes: maximumResponseBytes)
            return .completed(
                transcript: transcript,
                externalOperationID: job.requestID,
                status: "completed"
            )
        case .openRouterWhisper:
            let transcript = try await providers.openRouter(model).transcribe(
                audioURL: url,
                episodeID: episodeID,
                languageHint: nil
            )
            try enforceBound(transcript, maximumResponseBytes: maximumResponseBytes)
            return .completed(
                transcript: transcript,
                externalOperationID: nil,
                status: "completed"
            )
        case .appleSpeech, .unsupported:
            throw CoreTranscriptTransportError.unsupportedProvider
        }
    }

    private func recover(
        context: TranscriptCapabilityContext,
        provider: TranscriptProvider,
        model: String,
        externalOperationID: String,
        maximumResponseBytes: UInt64
    ) async throws -> CoreTranscriptTransportObservation {
        guard let episodeID = context.episodeId.uuid else {
            throw CoreTranscriptTransportError.invalidRequest
        }
        switch provider {
        case .assemblyAi:
            let job = AssemblyAIJob(
                transcriptID: externalOperationID,
                episodeID: episodeID,
                createdAt: Date(),
                languageHint: nil,
                speechModels: [model]
            )
            switch try await providers.assemblyAI.observe(
                job,
                maximumResponseBytes: maximumResponseBytes
            ) {
            case .pending(let status):
                return .providerPending(status: status, retryAfterMilliseconds: nil)
            case .completed(let transcript):
                return .completed(
                    transcript: transcript,
                    externalOperationID: externalOperationID,
                    status: "completed"
                )
            }
        case .elevenLabsScribe:
            let client = providers.elevenLabs(model)
            let transcript = try await client.result(for: ScribeJob(
                requestID: externalOperationID,
                episodeID: episodeID,
                createdAt: Date(),
                languageHint: nil,
                inlineResult: nil
            ))
            try enforceBound(transcript, maximumResponseBytes: maximumResponseBytes)
            return .completed(
                transcript: transcript,
                externalOperationID: externalOperationID,
                status: "completed"
            )
        case .openRouterWhisper, .appleSpeech, .unsupported:
            throw CoreTranscriptTransportError.providerRecoveryUnavailable
        }
    }
}
