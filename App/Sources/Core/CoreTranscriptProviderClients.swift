import Foundation

protocol CoreAssemblyAITranscribing: Sendable {
    func submit(
        audioURL: URL,
        episodeID: UUID,
        speechModels: [String],
        speakerLabels: Bool,
        languageDetection: Bool,
        languageHint: String?
    ) async throws -> AssemblyAIJob

    func observe(
        _ job: AssemblyAIJob,
        maximumResponseBytes: UInt64
    ) async throws -> AssemblyAIStatusObservation
}

protocol CoreElevenLabsTranscribing: Sendable {
    func submit(
        audioURL: URL,
        episodeID: UUID,
        languageHint: String?
    ) async throws -> ScribeJob

    func result(for job: ScribeJob) async throws -> Transcript
}

protocol CoreOpenRouterTranscribing: Sendable {
    func transcribe(
        audioURL: URL,
        episodeID: UUID,
        languageHint: String?
    ) async throws -> Transcript
}

protocol CoreAppleSpeechTranscribing: Sendable {
    func transcribe(
        audioFileURL: URL,
        episodeID: UUID,
        languageHint: String?
    ) async throws -> Transcript
}

extension AssemblyAITranscriptClient: CoreAssemblyAITranscribing {}
extension ElevenLabsScribeClient: CoreElevenLabsTranscribing {}
extension OpenRouterWhisperClient: CoreOpenRouterTranscribing {}
extension AppleNativeSTTClient: CoreAppleSpeechTranscribing {}

struct CoreTranscriptProviderClients: Sendable {
    let assemblyAI: any CoreAssemblyAITranscribing
    let elevenLabs: @Sendable (String) -> any CoreElevenLabsTranscribing
    let openRouter: @Sendable (String) -> any CoreOpenRouterTranscribing
    let appleSpeech: any CoreAppleSpeechTranscribing

    static func live(session: URLSession) -> Self {
        Self(
            assemblyAI: AssemblyAITranscriptClient(session: session),
            elevenLabs: { ElevenLabsScribeClient(modelID: $0, session: session) },
            openRouter: { OpenRouterWhisperClient(model: $0, session: session) },
            appleSpeech: AppleNativeSTTClient()
        )
    }
}
