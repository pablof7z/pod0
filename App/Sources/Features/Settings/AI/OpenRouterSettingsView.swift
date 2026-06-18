import SwiftUI

/// Main hub screen for OpenRouter configuration.
/// Covers model selection, BYOK / manual key management, and connection status.
struct OpenRouterSettingsView: View {
    @Environment(AppStateStore.self) private var store
    /// Accessed directly so SwiftUI tracks `kernel.snapshot` and re-renders
    /// whenever `open_router_key_present` changes (e.g. after a key save or
    /// delete dispatches `set_provider_api_keys`). Reading through
    /// `store.kernel` would bypass observation tracking because `kernel` is
    /// `@ObservationIgnored` on `AppStateStore`.
    @Environment(KernelModel.self) private var kernelModel

    @State private var settings: Settings = Settings()
    @State private var manualAPIKey = ""
    @State private var isConnectingBYOK = false
    @State private var credentialMessage: String?
    @State private var credentialError: String?
    @State private var byokConnect = BYOKConnectService()
    @State private var isValidatingKey = false
    @State private var keyInfo: OpenRouterKeyInfo?
    private let validationService = OpenRouterKeyValidationService()

    var body: some View {
        Form {
            connectionSection
        }
        .listStyle(.insetGrouped)
        .navigationTitle("OpenRouter")
        .navigationBarTitleDisplayMode(.inline)
        .onAppear {
            settings = store.state.settings
            refreshCredentialState()
        }
        .onChange(of: settings) { _, new in store.updateSettings(new) }
        .animation(AppTheme.Animation.spring, value: credentialMessage)
        .animation(AppTheme.Animation.spring, value: credentialError)
        .animation(AppTheme.Animation.spring, value: keyInfo?.label)
        .toolbar {
            ToolbarItem(placement: .navigationBarTrailing) {
                Button("Save") { saveManualKey() }
                    .disabled(manualAPIKey.isBlank)
            }
        }
    }

    // MARK: - Connection section

    private var connectionSection: some View {
        Section {
            // Status row
            Label(statusTitle, systemImage: statusIcon)
                .foregroundStyle(statusColor)
                .accessibilityIdentifier("openrouter-status-label")

            // BYOK button
            Button {
                Task { await connectWithBYOK() }
            } label: {
                HStack {
                    Label(
                        isConnectingBYOK ? "Connecting..." : byokButtonTitle,
                        systemImage: "key.viewfinder"
                    )
                    if isConnectingBYOK {
                        Spacer()
                        ProgressView()
                    }
                }
            }
            .buttonStyle(.glassProminent)
            .disabled(isConnectingBYOK)

            // Manual key field
            RevealableAPIKeyField(
                "Paste OpenRouter API key",
                text: $manualAPIKey,
                accessibilityIdentifier: "openrouter-api-key-field"
            )
            .onSubmit { saveManualKey() }

            // Disconnect (only when key stored)
            if hasStoredOpenRouterKey {
                Button(role: .destructive) {
                    disconnectOpenRouter()
                } label: {
                    Label("Disconnect", systemImage: "trash")
                }
                .accessibilityIdentifier("openrouter-disconnect-button")
            }

            // Validate stored key
            if hasStoredOpenRouterKey {
                Button {
                    Task { await validateStoredKey() }
                } label: {
                    HStack {
                        Label(
                            isValidatingKey ? "Validating…" : "Validate Key",
                            systemImage: "checkmark.shield"
                        )
                        if isValidatingKey {
                            Spacer()
                            ProgressView()
                        }
                    }
                }
                .disabled(isValidatingKey)
            }

            // Key info card (shown after successful validation)
            if let keyInfo {
                OpenRouterKeyInfoCard(info: keyInfo)
                    .listRowInsets(AppTheme.Layout.cardRowInsetsXS)
                    .listRowBackground(Color.clear)
                    .transition(.opacity.combined(with: .move(edge: .top)))
            }

            // Flash messages
            if let credentialMessage {
                Text(credentialMessage)
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.secondary)
                    .transition(.opacity.combined(with: .move(edge: .top)))
            }

            if let credentialError {
                Text(credentialError)
                    .inlineErrorText()
                    .transition(.opacity.combined(with: .move(edge: .top)))
            }
        } header: {
            Text("Connection")
        } footer: {
            Text("BYOK opens byok.f7z.io for consent and stores the returned key in Keychain. Manual keys are also saved only in Keychain.")
        }
    }

    // MARK: - Status helpers

    private var hasStoredOpenRouterKey: Bool {
        kernelModel.settings.openRouterKeyPresent
    }

    private var statusTitle: String {
        guard hasStoredOpenRouterKey else {
            return settings.openRouterCredentialSource == .none ? "Not connected" : "Reconnect required"
        }
        switch settings.openRouterCredentialSource {
        case .byok:   return "Connected with BYOK"
        case .manual: return "Manual key saved"
        case .none:   return "Key stored"
        }
    }

    private var statusIcon: String {
        hasStoredOpenRouterKey ? "checkmark.seal.fill" : "xmark.seal"
    }

    private var statusColor: Color {
        hasStoredOpenRouterKey ? .green : .secondary
    }

    private var byokButtonTitle: String {
        settings.openRouterCredentialSource == .byok ? "Reconnect BYOK" : "Connect with BYOK"
    }

    // MARK: - Credential actions

    private func connectWithBYOK() async {
        credentialError = nil
        credentialMessage = nil
        isConnectingBYOK = true
        defer { isConnectingBYOK = false }

        do {
            let token = try await byokConnect.connectOpenRouter()
            try OpenRouterCredentialStore.saveAPIKey(token.apiKey)
            settings.markOpenRouterBYOK(keyID: token.keyID, keyLabel: token.keyLabel)
            store.updateSettings(settings)
            manualAPIKey = ""
            refreshCredentialState()
            credentialMessage = "OpenRouter connected with BYOK."
            Haptics.success()
        } catch {
            credentialError = error.localizedDescription
        }
    }

    private func saveManualKey() {
        credentialError = nil
        credentialMessage = nil
        do {
            try OpenRouterCredentialStore.saveAPIKey(manualAPIKey)
            settings.markOpenRouterManual()
            store.updateSettings(settings)
            manualAPIKey = ""
            refreshCredentialState()
            credentialMessage = "OpenRouter key saved in Keychain."
            Haptics.success()
        } catch {
            credentialError = "OpenRouter key could not be saved."
        }
    }

    private func disconnectOpenRouter() {
        credentialError = nil
        credentialMessage = nil
        keyInfo = nil
        do {
            try OpenRouterCredentialStore.deleteAPIKey()
            settings.clearOpenRouterCredential()
            store.updateSettings(settings)
            manualAPIKey = ""
            refreshCredentialState()
            credentialMessage = "OpenRouter disconnected."
            Haptics.success()
        } catch {
            credentialError = "OpenRouter key could not be deleted."
        }
    }

    private func refreshCredentialState() {
        // Re-report STT key presence so the kernel's STT fallback policy
        // recomputes `settings.effectiveSttProvider` after a save/delete.
        store.syncSTTKeysPresent()
        // Re-push LLM provider API keys to the kernel so it has the latest
        // Keychain values after a save/delete.
        store.kernelSetProviderApiKeys()
    }

    private func validateStoredKey() async {
        credentialError = nil
        credentialMessage = nil
        keyInfo = nil
        isValidatingKey = true
        defer { isValidatingKey = false }

        do {
            store.kernelSetProviderApiKeys()
            keyInfo = try await validationService.validateStoredKey()
            Haptics.success()
        } catch {
            credentialError = error.localizedDescription
            Haptics.warning()
        }
    }
}
