import Foundation

extension SettingsSnapshot {
    var openRouterSource: OpenRouterCredentialSource {
        OpenRouterCredentialSource(rawValue: openRouterCredentialSource) ?? .none
    }

    var elevenLabsSource: ElevenLabsCredentialSource {
        ElevenLabsCredentialSource(rawValue: elevenLabsCredentialSource) ?? .none
    }

    var ollamaSource: OllamaCredentialSource {
        OllamaCredentialSource(rawValue: ollamaCredentialSource) ?? .none
    }

    var selectedSTTProvider: STTProvider {
        STTProvider(rawValue: sttProvider) ?? .appleNative
    }

    var resolvedSTTProvider: STTProvider {
        STTProvider(rawValue: effectiveSttProvider) ?? .appleNative
    }

    func hasLoadedKey(for provider: STTProvider) -> Bool {
        switch provider {
        case .elevenLabsScribe:
            return elevenLabsKeyPresent
        case .assemblyAI:
            return assemblyAIKeyPresent
        case .openRouterWhisper:
            return openRouterKeyPresent
        case .appleNative:
            return true
        }
    }
}
