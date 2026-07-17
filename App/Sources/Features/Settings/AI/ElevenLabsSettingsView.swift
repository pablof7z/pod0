import SwiftUI

/// ElevenLabs connection screen — hero card plus the shared
/// `ProviderConnectionSection` state machine, with account validation
/// surfacing the subscription/quota card.
struct ElevenLabsSettingsView: View {
    @Environment(AppStateStore.self) private var store

    private let validationService = ElevenLabsKeyValidationService()

    var body: some View {
        Form {
            heroSection
            ProviderConnectionSection(config: config)
        }
        .navigationTitle("ElevenLabs")
        .navigationBarTitleDisplayMode(.inline)
    }

    private var heroSection: some View {
        Section {
            ElevenLabsHeroCard(
                connectionState: ElevenLabsConnectionState.derive(
                    source: store.state.settings.elevenLabsCredentialSource,
                    hasKey: ElevenLabsCredentialStore.hasAPIKey()
                ),
                keyLabel: store.state.settings.elevenLabsBYOKKeyLabel,
                connectedAt: store.state.settings.elevenLabsConnectedAt
            )
            .listRowBackground(Color.clear)
            .listRowInsets(AppTheme.Layout.cardRowInsetsSM)
        }
    }

    private var config: ProviderConnectionConfig {
        ProviderConnectionConfig(
            providerName: "ElevenLabs",
            keyPlaceholder: "Paste ElevenLabs API key",
            footer: "ElevenLabs powers agent voices, TTS previews, and realtime voice notes. BYOK opens byok.f7z.io for consent; all keys live only in Keychain.",
            hasKey: { ElevenLabsCredentialStore.hasAPIKey() },
            saveKey: { try ElevenLabsCredentialStore.saveAPIKey($0) },
            deleteKey: { try ElevenLabsCredentialStore.deleteAPIKey() },
            connectBYOK: { try await $0.connectElevenLabs() },
            keySource: { [store] in
                switch store.state.settings.elevenLabsCredentialSource {
                case .byok: .byok
                case .manual: .manual
                case .none: .none
                }
            },
            didConnectBYOK: { [store] token in
                var settings = store.state.settings
                settings.markElevenLabsBYOK(keyID: token.keyID, keyLabel: token.keyLabel)
                store.updateSettings(settings)
            },
            didSaveManual: { [store] in
                var settings = store.state.settings
                settings.markElevenLabsManual()
                store.updateSettings(settings)
            },
            didDisconnect: { [store] in
                var settings = store.state.settings
                settings.clearElevenLabsCredential()
                store.updateSettings(settings)
            },
            validation: ProviderValidation(
                idleLabel: "Validate Key",
                busyLabel: "Validating…",
                icon: "checkmark.shield",
                run: { [validationService] in
                    guard let apiKey = try ElevenLabsCredentialStore.apiKey() else {
                        throw OpenRouterKeyValidationError.noStoredKey
                    }
                    let info = try await validationService.validate(apiKey: apiKey)
                    return ProviderValidationResult(card: AnyView(ElevenLabsKeyInfoCard(info: info)))
                }
            )
        )
    }
}
