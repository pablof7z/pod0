import SwiftUI

struct ElevenLabsSettingsView: View {
    @Environment(AppStateStore.self) private var store

    @State private var settings: Settings = Settings()
    @State private var manualAPIKey = ""
    @State private var hasStoredKey = false
    @State private var isConnectingBYOK = false
    @State private var credentialMessage: String?
    @State private var credentialError: String?
    @State private var byokConnect = BYOKConnectService()
    @State private var isValidatingKey = false
    @State private var keyInfo: ElevenLabsKeyInfo?
    private let validationService = ElevenLabsKeyValidationService()

    var body: some View {
        Form {
            heroSection
            connectionSection
        }
        .navigationTitle("ElevenLabs")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .navigationBarTrailing) {
                Button("Save") { saveManualKey() }
                    .disabled(manualAPIKey.isBlank)
            }
        }
        .onAppear {
            settings = store.state.settings
            refreshCredentialState()
        }
        .onChange(of: settings) { _, new in store.updateSettings(new) }
    }

    // MARK: - Sections

    private var heroSection: some View {
        Section {
            ElevenLabsHeroCard(
                connectionState: connectionState,
                keyLabel: settings.elevenLabsBYOKKeyLabel,
                connectedAt: settings.elevenLabsConnectedAt
            )
            .listRowBackground(Color.clear)
            .listRowInsets(AppTheme.Layout.cardRowInsetsSM)
        }
    }

    private var connectionSection: some View {
        ElevenLabsConnectionSection(
            statusTitle: statusTitle,
            statusIcon: statusIcon,
            byokButtonTitle: byokButtonTitle,
            isConnectingBYOK: isConnectingBYOK,
            isValidatingKey: isValidatingKey,
            hasStoredKey: hasStoredKey,
            keyInfo: keyInfo,
            credentialMessage: credentialMessage,
            credentialError: credentialError,
            manualAPIKey: $manualAPIKey,
            onConnectBYOK: { Task { await connectWithBYOK() } },
            onDisconnect: disconnectElevenLabs,
            onValidateKey: { Task { await validateStoredKey() } }
        )
    }

    // MARK: - Derived state

    private var connectionState: ElevenLabsConnectionState {
        ElevenLabsConnectionState.derive(
            source: settings.elevenLabsCredentialSource,
            hasKey: hasStoredKey
        )
    }

    private var byokButtonTitle: String {
        settings.elevenLabsCredentialSource == .byok ? "Reconnect BYOK" : "Connect with BYOK"
    }

    private var statusTitle: String {
        guard hasStoredKey else {
            return settings.elevenLabsCredentialSource == .none ? "Not connected" : "Reconnect required"
        }
        switch settings.elevenLabsCredentialSource {
        case .byok:   return "Connected with BYOK"
        case .manual: return "Manual key saved"
        case .none:   return "Key stored"
        }
    }

    private var statusIcon: String {
        guard hasStoredKey else {
            return settings.elevenLabsCredentialSource == .none ? "waveform.circle" : "exclamationmark.triangle"
        }
        return "waveform.circle.fill"
    }

    // MARK: - Credential actions

    private func connectWithBYOK() async {
        credentialError = nil
        credentialMessage = nil
        isConnectingBYOK = true
        defer { isConnectingBYOK = false }

        do {
            let token = try await byokConnect.connectElevenLabs()
            try ElevenLabsCredentialStore.saveAPIKey(token.apiKey)
            settings.markElevenLabsBYOK(keyID: token.keyID, keyLabel: token.keyLabel)
            store.updateSettings(settings)
            manualAPIKey = ""
            refreshCredentialState()
            credentialMessage = "ElevenLabs connected with BYOK."
            Haptics.success()
        } catch {
            credentialError = error.localizedDescription
        }
    }

    private func saveManualKey() {
        credentialError = nil
        credentialMessage = nil
        do {
            try ElevenLabsCredentialStore.saveAPIKey(manualAPIKey)
            settings.markElevenLabsManual()
            store.updateSettings(settings)
            manualAPIKey = ""
            refreshCredentialState()
            credentialMessage = "ElevenLabs key saved in Keychain."
            Haptics.success()
        } catch {
            credentialError = "ElevenLabs key could not be saved."
        }
    }

    private func disconnectElevenLabs() {
        credentialError = nil
        credentialMessage = nil
        keyInfo = nil
        do {
            try ElevenLabsCredentialStore.deleteAPIKey()
            settings.clearElevenLabsCredential()
            store.updateSettings(settings)
            manualAPIKey = ""
            refreshCredentialState()
            credentialMessage = "ElevenLabs disconnected."
            Haptics.success()
        } catch {
            credentialError = "ElevenLabs key could not be deleted."
        }
    }

    private func refreshCredentialState() {
        hasStoredKey = ElevenLabsCredentialStore.hasAPIKey()
        if !hasStoredKey { keyInfo = nil }
    }

    private func validateStoredKey() async {
        credentialError = nil
        credentialMessage = nil
        keyInfo = nil
        isValidatingKey = true
        defer { isValidatingKey = false }

        do {
            guard let apiKey = try ElevenLabsCredentialStore.apiKey() else {
                credentialError = "No stored key found."
                return
            }
            keyInfo = try await validationService.validate(apiKey: apiKey)
            Haptics.success()
        } catch {
            credentialError = error.localizedDescription
            Haptics.warning()
        }
    }
}
