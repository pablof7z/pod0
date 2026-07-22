import Foundation
import Pod0Core

/// Supplies bounded platform facts to the Rust transcript planner. It does not
/// decide admission, fallback, retry, or workflow state.
enum NativeTranscriptWorkflowConfiguration {
    /// Bounds native capability announcements without taking ownership of
    /// admission policy. Rust still decides whether an announced opportunity
    /// creates, resumes, or leaves a workflow unchanged.
    static func hasAutomaticExecutionOpportunity(
        for episode: Episode,
        configuration: TranscriptWorkflowConfiguration
    ) -> Bool {
        guard case .none = episode.transcriptState else { return false }
        if episode.publisherTranscriptURL != nil, configuration.autoPublisherEnabled {
            return true
        }
        return configuration.autoProviderEnabled && configuration.credentialAvailable
    }

    static func make(
        episode: Episode,
        settings: Settings,
        provider explicitProvider: STTProvider? = nil
    ) -> TranscriptWorkflowConfiguration {
        let provider = explicitProvider ?? settings.sttProvider
        let localAudio = episode.downloadState.localFileURL.flatMap { url in
            FileManager.default.fileExists(atPath: url.path) ? url.absoluteString : nil
        }
        return TranscriptWorkflowConfiguration(
            provider: provider.coreValue,
            model: model(for: provider, settings: settings),
            localAudioUrl: localAudio,
            credentialAvailable: credentialAvailable(for: provider),
            autoPublisherEnabled: settings.autoIngestPublisherTranscripts,
            autoProviderEnabled: settings.autoFallbackToScribe
        )
    }

    static func make(
        episode: Episode,
        settings: Settings,
        provider: Pod0Core.TranscriptProvider,
        model: String
    ) -> TranscriptWorkflowConfiguration {
        let nativeProvider = STTProvider(coreValue: provider)
        let localAudio = episode.downloadState.localFileURL.flatMap { url in
            FileManager.default.fileExists(atPath: url.path) ? url.absoluteString : nil
        }
        return TranscriptWorkflowConfiguration(
            provider: provider,
            model: model,
            localAudioUrl: localAudio,
            credentialAvailable: nativeProvider.map(credentialAvailable) ?? false,
            autoPublisherEnabled: settings.autoIngestPublisherTranscripts,
            autoProviderEnabled: settings.autoFallbackToScribe
        )
    }

    static func model(for provider: STTProvider, settings: Settings) -> String {
        switch provider {
        case .elevenLabsScribe: settings.elevenLabsSTTModel
        case .assemblyAI: settings.assemblyAISTTModel
        case .openRouterWhisper: settings.openRouterWhisperModel
        case .appleNative: "apple-native-v1"
        }
    }

    static func credentialAvailable(for provider: STTProvider) -> Bool {
        switch provider {
        case .elevenLabsScribe:
            return credential(try? ElevenLabsCredentialStore.apiKey())
        case .assemblyAI:
            return credential(try? AssemblyAICredentialStore.apiKey())
        case .openRouterWhisper:
            return credential(try? OpenRouterCredentialStore.apiKey())
        case .appleNative:
            return true
        }
    }

    private static func credential(_ value: String?) -> Bool {
        guard let resolved = value else { return false }
        return !resolved.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }
}

extension STTProvider {
    var coreValue: Pod0Core.TranscriptProvider {
        switch self {
        case .assemblyAI: .assemblyAi
        case .elevenLabsScribe: .elevenLabsScribe
        case .openRouterWhisper: .openRouterWhisper
        case .appleNative: .appleSpeech
        }
    }

    init?(coreValue: Pod0Core.TranscriptProvider) {
        switch coreValue {
        case .assemblyAi: self = .assemblyAI
        case .elevenLabsScribe: self = .elevenLabsScribe
        case .openRouterWhisper: self = .openRouterWhisper
        case .appleSpeech: self = .appleNative
        case .unsupported: return nil
        }
    }
}
