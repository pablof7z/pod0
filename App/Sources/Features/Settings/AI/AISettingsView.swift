import SwiftUI

struct AIProvidersSettingsView: View {
    @Environment(AppStateStore.self) private var store
    @ObservedObject private var ledger = CostLedger.shared

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
            Text("Use BYOK Vault to approve several keys at once, or open a provider for manual keys and validation. Choose each role's provider and model in Models.")
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

    private var openRouterStatus: String {
        guard OpenRouterCredentialStore.hasAPIKey() else {
            return settings.openRouterCredentialSource == .none ? "Not set up" : "Reconnect"
        }
        switch settings.openRouterCredentialSource {
        case .byok:   return "BYOK"
        case .manual: return "Manual"
        case .none:   return "Connected"
        }
    }

    private var elevenLabsStatus: String {
        guard ElevenLabsCredentialStore.hasAPIKey() else {
            return settings.elevenLabsCredentialSource == .none ? "Not set up" : "Reconnect"
        }
        switch settings.elevenLabsCredentialSource {
        case .byok:   return "BYOK"
        case .manual: return "Manual"
        case .none:   return "Connected"
        }
    }

    private var assemblyAIStatus: String {
        AssemblyAICredentialStore.hasAPIKey() ? "Connected" : "Not set up"
    }

    private var perplexityStatus: String {
        if PerplexityCredentialStore.hasAPIKey() { return "Connected" }
        if OpenRouterCredentialStore.hasAPIKey() { return "Via OpenRouter" }
        return "Not set up"
    }

    private var ollamaStatus: String {
        guard OllamaCredentialStore.hasAPIKey() else {
            return settings.ollamaCredentialSource == .none ? "Not set up" : "Reconnect"
        }
        switch settings.ollamaCredentialSource {
        case .byok:   return "BYOK"
        case .manual: return "Manual"
        case .none:   return "Connected"
        }
    }

    private var youtubeStatus: String {
        settings.youtubeExtractorURL != nil ? "Configured" : "Not set up"
    }

    private var usageSummary: String? {
        guard !ledger.records.isEmpty else { return nil }
        let total = ledger.records.reduce(0) { $0 + $1.costUSD }
        return "\(ledger.records.count) calls · \(CostFormatter.usd(total))"
    }
}

struct AISettingsView: View {
    var body: some View {
        AIProvidersSettingsView()
    }
}

struct PerplexitySettingsView: View {
    @State private var manualAPIKey = ""
    @State private var hasStoredKey = false
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
        .onAppear(perform: refreshCredentialState)
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

            Button {
                Task { await connectWithBYOK() }
            } label: {
                HStack {
                    Label(isConnectingBYOK ? "Connecting..." : "Connect with BYOK", systemImage: "key.viewfinder")
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

    private var statusTitle: String {
        hasStoredKey ? "Connected" : "Not connected"
    }

    private var statusIcon: String {
        hasStoredKey ? "checkmark.seal.fill" : "xmark.seal"
    }

    private var statusColor: Color {
        hasStoredKey ? .green : .secondary
    }

    private func connectWithBYOK() async {
        credentialError = nil
        credentialMessage = nil
        isConnectingBYOK = true
        defer { isConnectingBYOK = false }

        do {
            let token = try await byokConnect.connectPerplexity()
            try PerplexityCredentialStore.saveAPIKey(token.apiKey)
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
        hasStoredKey = PerplexityCredentialStore.hasAPIKey()
    }
}
