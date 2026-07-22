import Foundation
import Pod0Core

actor LiveCoreTranscriptTransport: CoreTranscriptTransporting {
    let session: URLSession
    private let assemblyAI: AssemblyAITranscriptClient
    private let appleSTT: AppleNativeSTTClient

    init(
        session: URLSession = .shared,
        assemblyAI: AssemblyAITranscriptClient = AssemblyAITranscriptClient(),
        appleSTT: AppleNativeSTTClient = AppleNativeSTTClient()
    ) {
        self.session = session
        self.assemblyAI = assemblyAI
        self.appleSTT = appleSTT
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
            let transcript = try await appleSTT.transcribe(
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
            let job = try await assemblyAI.submit(
                audioURL: url,
                episodeID: episodeID,
                speechModels: [model],
                speakerLabels: true,
                languageDetection: true
            )
            return .providerAccepted(externalOperationID: job.transcriptID, status: "queued")
        case .elevenLabsScribe:
            let client = ElevenLabsScribeClient(modelID: model)
            let job = try await client.submit(audioURL: url, episodeID: episodeID)
            let transcript = try await client.pollResult(job)
            try enforceBound(transcript, maximumResponseBytes: maximumResponseBytes)
            return .completed(
                transcript: transcript,
                externalOperationID: job.requestID,
                status: "completed"
            )
        case .openRouterWhisper:
            let transcript = try await OpenRouterWhisperClient(model: model).transcribe(
                audioURL: url,
                episodeID: episodeID
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
            switch try await assemblyAI.observe(job, maximumResponseBytes: maximumResponseBytes) {
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
            let client = ElevenLabsScribeClient(modelID: model)
            let transcript = try await client.pollResult(ScribeJob(
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
