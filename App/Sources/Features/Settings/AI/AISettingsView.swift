import SwiftUI

struct AIProvidersSettingsView: View {
    @Environment(AppStateStore.self) private var store
    @ObservedObject private var ledger = CostLedger.shared
    @State private var localDownloadedModelCount = 0

    var body: some View {
        ZStack {
            Color(.systemGroupedBackground)
                .ignoresSafeArea()

            List {
                BYOKUnifiedProviderSection()
                providersSection
                usageSection
            }
            .listStyle(.insetGrouped)
            .scrollContentBackground(.hidden)
        }
        .navigationTitle("Providers")
        .navigationBarTitleDisplayMode(.inline)
        .task { await refreshLocalModelStatus() }
    }

    private var providersSection: some View {
        Section {
            NavigationLink {
                OpenRouterSettingsView()
            } label: {
                SettingsRow(
                    icon: "key.viewfinder",
                    tint: .indigo,
                    title: "OpenRouter",
                    value: openRouterStatus
                )
            }
            .accessibilityIdentifier("ai-provider-openrouter")

            NavigationLink {
                ElevenLabsSettingsView()
            } label: {
                SettingsRow(
                    icon: "waveform",
                    tint: AppTheme.Brand.elevenLabsTint,
                    title: "ElevenLabs",
                    value: elevenLabsStatus
                )
            }

            NavigationLink {
                AssemblyAISettingsView()
            } label: {
                SettingsRow(
                    icon: "waveform.badge.mic",
                    tint: .purple,
                    title: "AssemblyAI",
                    value: assemblyAIStatus
                )
            }

            NavigationLink {
                PerplexitySettingsView()
            } label: {
                SettingsRow(
                    icon: "magnifyingglass.circle.fill",
                    tint: .teal,
                    title: "Perplexity",
                    value: perplexityStatus
                )
            }

            NavigationLink {
                OllamaSettingsView()
            } label: {
                SettingsRow(
                    icon: "cloud.fill",
                    tint: .green,
                    title: "Ollama Cloud",
                    value: ollamaStatus
                )
            }

            NavigationLink {
                LocalModelsSettingsView()
            } label: {
                SettingsRow(
                    icon: "iphone.gen3",
                    tint: .green,
                    title: "Local",
                    value: localStatus
                )
            }

            NavigationLink {
                YouTubeSettingsView()
            } label: {
                SettingsRow(
                    icon: "play.rectangle.fill",
                    tint: .red,
                    title: "YouTube Ingestion",
                    value: youtubeStatus
                )
            }
        } header: {
            Text("Connections")
        } footer: {
            Text("Use BYOK Vault to approve several keys at once, or open a provider for manual keys and validation. Download on-device models under Local. Choose each role's provider and model in Models.")
        }
    }

    private var usageSection: some View {
        Section("Usage") {
            NavigationLink {
                UsageCostSettingsView()
            } label: {
                SettingsRow(
                    icon: "dollarsign.circle.fill",
                    tint: .green,
                    title: "Usage & Cost",
                    value: usageSummary
                )
            }
        }
    }

    // MARK: - Derived

    private var settings: Settings { store.state.settings }
    private var kernelSettings: SettingsSnapshot { store.kernel?.settings ?? SettingsSnapshot() }

    private var openRouterSource: OpenRouterCredentialSource {
        let source = kernelSettings.openRouterSource
        return source == .none ? settings.openRouterCredentialSource : source
    }

    private var elevenLabsSource: ElevenLabsCredentialSource {
        let source = kernelSettings.elevenLabsSource
        return source == .none ? settings.elevenLabsCredentialSource : source
    }

    private var ollamaSource: OllamaCredentialSource {
        let source = kernelSettings.ollamaSource
        return source == .none ? settings.ollamaCredentialSource : source
    }

    private var assemblyAISource: AssemblyAICredentialSource {
        let source = kernelSettings.assemblyAISource
        return source == .none ? settings.assemblyAICredentialSource : source
    }

    private var perplexitySource: PerplexityCredentialSource {
        let source = kernelSettings.perplexitySource
        return source == .none ? settings.perplexityCredentialSource : source
    }

    private var openRouterStatus: String {
        guard kernelSettings.openRouterKeyPresent else {
            return openRouterSource == .none ? "Not set up" : "Reconnect"
        }
        switch openRouterSource {
        case .byok:   return "BYOK"
        case .manual: return "Manual"
        case .none:   return "Connected"
        }
    }

    private var elevenLabsStatus: String {
        guard kernelSettings.elevenLabsKeyPresent else {
            return elevenLabsSource == .none ? "Not set up" : "Reconnect"
        }
        switch elevenLabsSource {
        case .byok:   return "BYOK"
        case .manual: return "Manual"
        case .none:   return "Connected"
        }
    }

    private var assemblyAIStatus: String {
        providerCredentialStatus(
            source: assemblyAISource.rawValue,
            hasKey: kernelSettings.assemblyAIKeyPresent
        )
    }

    private var perplexityStatus: String {
        if kernelSettings.perplexityKeyPresent {
            return providerCredentialStatus(source: perplexitySource.rawValue, hasKey: true)
        }
        if perplexitySource != .none { return "Reconnect" }
        if kernelSettings.openRouterKeyPresent { return "Via OpenRouter" }
        return "Not set up"
    }

    private var ollamaStatus: String {
        guard kernelSettings.ollamaKeyPresent else {
            return ollamaSource == .none ? "Not set up" : "Reconnect"
        }
        switch ollamaSource {
        case .byok:   return "BYOK"
        case .manual: return "Manual"
        case .none:   return "Connected"
        }
    }

    private var youtubeStatus: String {
        settings.youtubeExtractorURL != nil ? "Configured" : "Not set up"
    }

    private var localStatus: String {
        switch localDownloadedModelCount {
        case 0:  return "No models"
        case 1:  return "1 model"
        default: return "\(localDownloadedModelCount) models"
        }
    }

    private var usageSummary: String? {
        guard !ledger.records.isEmpty else { return nil }
        let total = ledger.records.reduce(0) { $0 + $1.costUSD }
        return "\(ledger.records.count) calls · \(CostFormatter.usd(total))"
    }

    private func providerCredentialStatus(source: String, hasKey: Bool) -> String {
        guard hasKey else { return source == "none" || source.isEmpty ? "Not set up" : "Reconnect" }
        switch source {
        case "byok":   return "BYOK"
        case "manual": return "Manual"
        default:       return "Connected"
        }
    }

    @MainActor
    private func refreshLocalModelStatus() async {
        let specs: [LocalModelSpec]
        switch await LocalModelCatalog.fetch() {
        case .loaded(let loadedSpecs):
            specs = loadedSpecs
        case .failed:
            specs = []
        }
        localDownloadedModelCount = specs.filter { LocalModelCatalog.isDownloaded($0.id) }.count
    }
}

struct AISettingsView: View {
    var body: some View {
        AIProvidersSettingsView()
    }
}

struct PerplexitySettingsView: View {
    @Environment(AppStateStore.self) private var store

    @State private var settings: Settings = Settings()
    @State private var manualAPIKey = ""
    @State private var isConnectingBYOK = false
    @State private var credentialMessage: String?
    @State private var credentialError: String?
    @State private var byokConnect = BYOKConnectService()

    var body: some View {
        Form {
            connectionSection
        }
        .listStyle(.insetGrouped)
        .navigationTitle("Perplexity")
        .navigationBarTitleDisplayMode(.inline)
        .onAppear {
            settings = store.state.settings
            refreshCredentialState()
        }
        .onChange(of: settings) { _, new in store.updateSettings(new) }
        .animation(AppTheme.Animation.spring, value: credentialMessage)
        .animation(AppTheme.Animation.spring, value: credentialError)
        .animation(AppTheme.Animation.spring, value: isConnectingBYOK)
        .toolbar {
            ToolbarItem(placement: .navigationBarTrailing) {
                Button("Save") { saveManualKey() }
                    .disabled(manualAPIKey.isBlank)
            }
        }
    }

    private var connectionSection: some View {
        Section {
            Label(statusTitle, systemImage: statusIcon)
                .foregroundStyle(statusColor)

            if settings.perplexityCredentialSource == .byok,
               let label = settings.perplexityBYOKKeyLabel,
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

            RevealableAPIKeyField("Paste Perplexity API key", text: $manualAPIKey)
                .onSubmit { saveManualKey() }

            if hasStoredKey {
                Button(role: .destructive) {
                    disconnect()
                } label: {
                    Label("Disconnect", systemImage: "trash")
                }
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
            Text("If you have an OpenRouter key configured, online search routes through OpenRouter automatically — no separate Perplexity key needed. A dedicated Perplexity key takes priority if both are set.")
        }
    }

    private var hasStoredKey: Bool {
        (store.kernel?.settings ?? SettingsSnapshot()).perplexityKeyPresent
    }

    private var statusTitle: String {
        guard hasStoredKey else {
            return settings.perplexityCredentialSource == .none ? "Not connected" : "Reconnect required"
        }
        switch settings.perplexityCredentialSource {
        case .byok:   return "Connected with BYOK"
        case .manual: return "Manual key saved"
        case .none:   return "Key stored"
        }
    }

    private var statusIcon: String {
        guard hasStoredKey else {
            return settings.perplexityCredentialSource == .none ? "xmark.seal" : "exclamationmark.triangle"
        }
        return "checkmark.seal.fill"
    }

    private var statusColor: Color {
        hasStoredKey ? .green : .secondary
    }

    private var byokButtonTitle: String {
        settings.perplexityCredentialSource == .byok ? "Reconnect BYOK" : "Connect with BYOK"
    }

    private func connectWithBYOK() async {
        credentialError = nil
        credentialMessage = nil
        isConnectingBYOK = true
        defer { isConnectingBYOK = false }

        do {
            let token = try await byokConnect.connectPerplexity()
            try PerplexityCredentialStore.saveAPIKey(token.apiKey)
            settings.markPerplexityBYOK(keyID: token.keyID, keyLabel: token.keyLabel)
            store.updateSettings(settings)
            manualAPIKey = ""
            refreshCredentialState()
            credentialMessage = "Perplexity connected with BYOK."
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
        do {
            try PerplexityCredentialStore.saveAPIKey(manualAPIKey)
            settings.markPerplexityManual()
            store.updateSettings(settings)
            manualAPIKey = ""
            refreshCredentialState()
            credentialMessage = "Perplexity key saved in Keychain."
            Haptics.success()
        } catch {
            credentialError = "Perplexity key could not be saved."
            Haptics.error()
        }
    }

    private func disconnect() {
        credentialError = nil
        credentialMessage = nil
        do {
            try PerplexityCredentialStore.deleteAPIKey()
            settings.clearPerplexityCredential()
            store.updateSettings(settings)
            manualAPIKey = ""
            refreshCredentialState()
            credentialMessage = "Perplexity disconnected."
            Haptics.success()
        } catch {
            credentialError = "Perplexity key could not be deleted."
            Haptics.error()
        }
    }

    private func refreshCredentialState() {
        store.kernelSetProviderApiKeys()
    }
}
