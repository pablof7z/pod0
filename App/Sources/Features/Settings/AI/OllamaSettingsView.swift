import SwiftUI

/// Ollama Cloud connection screen — shared `ProviderConnectionSection`
/// state machine plus the self-hosted endpoint override section.
struct OllamaSettingsView: View {
    @Environment(AppStateStore.self) private var store

    @State private var chatURLInput = ""

    private let catalog = OllamaModelCatalogService()

    var body: some View {
        Form {
            ProviderConnectionSection(config: config)
            endpointSection
        }
        .listStyle(.insetGrouped)
        .navigationTitle("Ollama Cloud")
        .navigationBarTitleDisplayMode(.inline)
        .onAppear { chatURLInput = store.state.settings.ollamaChatURL }
        .toolbar {
            ToolbarItem(placement: .navigationBarTrailing) {
                Button("Save") { commitChatURL() }
                    .disabled(chatURLInput.isBlank || chatURLInput == store.state.settings.ollamaChatURL)
            }
        }
    }

    // MARK: - Connection

    private var config: ProviderConnectionConfig {
        ProviderConnectionConfig(
            providerName: "Ollama",
            keyPlaceholder: "Paste Ollama API key",
            footer: "BYOK opens byok.f7z.io for consent and stores the returned Ollama key in Keychain. Manual keys are also saved only in Keychain.",
            hasKey: { OllamaCredentialStore.hasAPIKey() },
            saveKey: { try OllamaCredentialStore.saveAPIKey($0) },
            deleteKey: { try OllamaCredentialStore.deleteAPIKey() },
            connectBYOK: { try await $0.connectOllama() },
            keySource: { [store] in
                switch store.state.settings.ollamaCredentialSource {
                case .byok: .byok
                case .manual: .manual
                case .none: .none
                }
            },
            didConnectBYOK: { [store] token in
                var settings = store.state.settings
                settings.markOllamaBYOK(keyID: token.keyID, keyLabel: token.keyLabel)
                store.updateSettings(settings)
            },
            didSaveManual: { [store] in
                var settings = store.state.settings
                settings.markOllamaManual()
                store.updateSettings(settings)
            },
            didDisconnect: { [store] in
                var settings = store.state.settings
                settings.clearOllamaCredential()
                store.updateSettings(settings)
            },
            validation: ProviderValidation(
                idleLabel: "Check Available Models",
                busyLabel: "Checking models...",
                icon: "list.bullet.rectangle",
                run: { [catalog] in
                    let models = try await catalog.fetchModels()
                    return ProviderValidationResult(
                        caption: "\(models.count) Ollama Cloud models available for selection."
                    )
                }
            )
        )
    }

    // MARK: - Endpoint

    private var endpointSection: some View {
        Section {
            TextField(Settings.defaultOllamaChatURL, text: $chatURLInput)
                .keyboardType(.URL)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .onSubmit { commitChatURL() }

            if !chatURLInput.isBlank, URL(string: chatURLInput.trimmed) == nil {
                Text("Enter a valid URL (e.g. http://localhost:11434/api/chat)")
                    .inlineErrorText()
            }

            if store.state.settings.ollamaChatURL != Settings.defaultOllamaChatURL {
                Button(role: .destructive) {
                    chatURLInput = Settings.defaultOllamaChatURL
                    commitChatURL()
                } label: {
                    Label("Reset to Default", systemImage: "arrow.counterclockwise")
                }
            }
        } header: {
            Text("Endpoint")
        } footer: {
            Text("Default: \(Settings.defaultOllamaChatURL). Point to a local instance with http://localhost:11434/api/chat or any self-hosted URL. Invalid URLs fall back to the default.")
        }
    }

    private func commitChatURL() {
        let trimmed = chatURLInput.trimmed
        guard !trimmed.isBlank else { return }
        let validated = URL(string: trimmed) != nil ? trimmed : Settings.defaultOllamaChatURL
        chatURLInput = validated
        var settings = store.state.settings
        settings.ollamaChatURL = validated
        store.updateSettings(settings)
    }
}
