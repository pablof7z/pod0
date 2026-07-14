import SwiftUI

struct OllamaSettingsView: View {
    @Environment(AppStateStore.self) private var store

    @State private var settings = Settings()
    @State private var manualAPIKey = ""
    @State private var hasStoredKey = false
    @State private var isConnectingBYOK = false
    @State private var isValidating = false
    @State private var credentialMessage: String?
    @State private var credentialError: String?
    @State private var modelCount: Int?
    @State private var byokConnect = BYOKConnectService()
    @State private var chatURLInput = ""

    private let catalog = OllamaModelCatalogService()

    var body: some View {
        Form {
            connectionSection
            endpointSection
        }
        .listStyle(.insetGrouped)
        .navigationTitle("Ollama Cloud")
        .navigationBarTitleDisplayMode(.inline)
        .onAppear {
            settings = store.state.settings
            chatURLInput = settings.ollamaChatURL
            refreshCredentialState()
        }
        .onChange(of: settings) { _, new in store.updateSettings(new) }
        .animation(AppTheme.Animation.spring, value: credentialMessage)
        .animation(AppTheme.Animation.spring, value: credentialError)
        .animation(AppTheme.Animation.spring, value: modelCount)
        .animation(AppTheme.Animation.spring, value: isConnectingBYOK)
        .toolbar {
            ToolbarItem(placement: .navigationBarTrailing) {
                Button("Save") { commitChatURL() }
                    .disabled(chatURLInput.isBlank || chatURLInput == settings.ollamaChatURL)
            }
        }
    }

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

            if settings.ollamaChatURL != Settings.defaultOllamaChatURL {
                Button(role: .destructive) {
                    chatURLInput = Settings.defaultOllamaChatURL
                    settings.ollamaChatURL = Settings.defaultOllamaChatURL
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
        // Validate — fall back to default if not a valid URL
        let validated = URL(string: trimmed) != nil ? trimmed : Settings.defaultOllamaChatURL
        chatURLInput = validated
        settings.ollamaChatURL = validated
    }

    private var connectionSection: some View {
        Section {
            Label(statusTitle, systemImage: statusIcon)
                .foregroundStyle(statusColor)

            if settings.ollamaCredentialSource == .byok,
               let label = settings.ollamaBYOKKeyLabel,
               !label.isBlank {
                LabeledContent("BYOK key", value: label)
            }

            Button {
                Task { await connectWithBYOK() }
            } label: {
                HStack {
                    Label(isConnectingBYOK ? "Connecting..." : byokButtonTitle, systemImage: "key.viewfinder")
                    if isConnectingBYOK {
                        Spacer()
                        ProgressView()
                    }
                }
            }
            .disabled(isConnectingBYOK)

            RevealableAPIKeyField("Paste Ollama API key", text: $manualAPIKey)

            Button {
                saveManualKey()
            } label: {
                Label("Save Manual Key", systemImage: "square.and.arrow.down")
            }
            .disabled(manualAPIKey.isBlank)

            if hasStoredKey {
                Button {
                    Task { await validateStoredKey() }
                } label: {
                    HStack {
                        Label(isValidating ? "Checking models..." : "Check Available Models", systemImage: "list.bullet.rectangle")
                        if isValidating {
                            Spacer()
                            ProgressView()
                        }
                    }
                }
                .disabled(isValidating)

                Button(role: .destructive) {
                    disconnect()
                } label: {
                    Label("Disconnect", systemImage: "trash")
                }
            }

            if let modelCount {
                Text("\(modelCount) Ollama Cloud models available for selection.")
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.secondary)
            }

            if let credentialMessage {
                Text(credentialMessage)
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.secondary)
            }

            if let credentialError {
                Text(credentialError)
                    .inlineErrorText()
            }
        } header: {
            Text("Connection")
        } footer: {
            Text("BYOK opens byok.f7z.io for consent and stores the returned Ollama key in Keychain. Manual keys are also saved only in Keychain.")
        }
    }

    private var statusTitle: String {
        guard hasStoredKey else {
            return settings.ollamaCredentialSource == .none ? "Not connected" : "Reconnect required"
        }
        switch settings.ollamaCredentialSource {
        case .byok:   return "Connected with BYOK"
        case .manual: return "Manual key saved"
        case .none:   return "Key saved"
        }
    }

    private var statusIcon: String {
        hasStoredKey ? "checkmark.seal.fill" : "xmark.seal"
    }

    private var statusColor: Color {
        hasStoredKey ? .green : .secondary
    }

    private var byokButtonTitle: String {
        settings.ollamaCredentialSource == .byok ? "Reconnect BYOK" : "Connect with BYOK"
    }

    private func connectWithBYOK() async {
        credentialError = nil
        credentialMessage = nil
        modelCount = nil
        isConnectingBYOK = true
        defer { isConnectingBYOK = false }

        do {
            let token = try await byokConnect.connectOllama()
            try OllamaCredentialStore.saveAPIKey(token.apiKey)
            settings.markOllamaBYOK(keyID: token.keyID, keyLabel: token.keyLabel)
            store.updateSettings(settings)
            manualAPIKey = ""
            refreshCredentialState()
            credentialMessage = "Ollama Cloud connected with BYOK."
            Haptics.success()
        } catch BYOKConnectError.cancelled {
            Haptics.warning()
        } catch {
            credentialError = error.localizedDescription
            Haptics.error()
        }
    }

    private func saveManualKey() {
        credentialError = nil
        credentialMessage = nil
        modelCount = nil
        do {
            try OllamaCredentialStore.saveAPIKey(manualAPIKey)
            settings.markOllamaManual()
            store.updateSettings(settings)
            manualAPIKey = ""
            refreshCredentialState()
            credentialMessage = "Ollama key saved in Keychain."
            Haptics.success()
        } catch {
            credentialError = "Ollama key could not be saved."
            Haptics.error()
        }
    }

    private func disconnect() {
        credentialError = nil
        credentialMessage = nil
        modelCount = nil
        do {
            try OllamaCredentialStore.deleteAPIKey()
            settings.clearOllamaCredential()
            store.updateSettings(settings)
            manualAPIKey = ""
            refreshCredentialState()
            credentialMessage = "Ollama disconnected."
            Haptics.success()
        } catch {
            credentialError = "Ollama key could not be deleted."
            Haptics.error()
        }
    }

    private func refreshCredentialState() {
        hasStoredKey = OllamaCredentialStore.hasAPIKey()
    }

    private func validateStoredKey() async {
        credentialError = nil
        credentialMessage = nil
        modelCount = nil
        isValidating = true
        defer { isValidating = false }

        do {
            let models = try await catalog.fetchModels()
            modelCount = models.count
            Haptics.success()
        } catch {
            credentialError = error.localizedDescription
            Haptics.warning()
        }
    }
}
