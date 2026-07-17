import SwiftUI

/// OpenRouter connection screen — a thin wrapper around the shared
/// `ProviderConnectionSection` state machine, plus key validation that
/// surfaces the account/limits card.
struct OpenRouterSettingsView: View {
    @Environment(AppStateStore.self) private var store

    private let validationService = OpenRouterKeyValidationService()

    var body: some View {
        Form {
            ProviderConnectionSection(config: config)
        }
        .listStyle(.insetGrouped)
        .navigationTitle("OpenRouter")
        .navigationBarTitleDisplayMode(.inline)
    }

    private var config: ProviderConnectionConfig {
        ProviderConnectionConfig(
            providerName: "OpenRouter",
            keyPlaceholder: "Paste OpenRouter API key",
            footer: "BYOK opens byok.f7z.io for consent and stores the returned key in Keychain. Manual keys are also saved only in Keychain.",
            hasKey: { OpenRouterCredentialStore.hasAPIKey() },
            saveKey: { try OpenRouterCredentialStore.saveAPIKey($0) },
            deleteKey: { try OpenRouterCredentialStore.deleteAPIKey() },
            connectBYOK: { try await $0.connectOpenRouter() },
            keySource: { [store] in
                switch store.state.settings.openRouterCredentialSource {
                case .byok: .byok
                case .manual: .manual
                case .none: .none
                }
            },
            didConnectBYOK: { [store] token in
                var settings = store.state.settings
                settings.markOpenRouterBYOK(keyID: token.keyID, keyLabel: token.keyLabel)
                store.updateSettings(settings)
            },
            didSaveManual: { [store] in
                var settings = store.state.settings
                settings.markOpenRouterManual()
                store.updateSettings(settings)
            },
            didDisconnect: { [store] in
                var settings = store.state.settings
                settings.clearOpenRouterCredential()
                store.updateSettings(settings)
            },
            validation: ProviderValidation(
                idleLabel: "Validate Key",
                busyLabel: "Validating…",
                icon: "checkmark.shield",
                run: { [validationService] in
                    guard let apiKey = try OpenRouterCredentialStore.apiKey() else {
                        throw OpenRouterKeyValidationError.noStoredKey
                    }
                    let info = try await validationService.validate(apiKey: apiKey)
                    return ProviderValidationResult(card: AnyView(OpenRouterKeyInfoCard(info: info)))
                }
            )
        )
    }
}

enum OpenRouterKeyValidationError: LocalizedError {
    case noStoredKey
    var errorDescription: String? { "No stored key found." }
}
