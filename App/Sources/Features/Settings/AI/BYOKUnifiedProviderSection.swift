import SwiftUI

struct BYOKUnifiedProviderSection: View {
    @Environment(AppStateStore.self) private var store

    @State private var isConnecting = false
    @State private var message: String?
    @State private var errorMessage: String?
    @State private var byokConnect = BYOKConnectService()

    var body: some View {
        Section {
            Label(statusTitle, systemImage: connectedCount > 0 ? "checkmark.seal.fill" : "key.viewfinder")
                .foregroundStyle(connectedCount > 0 ? .green : .secondary)

            Button {
                Task { await connectWithBYOK() }
            } label: {
                HStack {
                    Label(isConnecting ? "Connecting..." : "Connect BYOK Vault", systemImage: "key.viewfinder")
                    if isConnecting {
                        Spacer()
                        ProgressView()
                    }
                }
            }
            .buttonStyle(.glassProminent)
            .disabled(isConnecting)

            ForEach(PodcastBYOKCredentialImporter.providers) { provider in
                LabeledContent {
                    Text(PodcastBYOKCredentialImporter.hasStoredKey(for: provider) ? "Connected" : "Not connected")
                        .foregroundStyle(.secondary)
                } label: {
                    Label(provider.displayName, systemImage: provider.iconName)
                }
            }

            if let message {
                Text(message)
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.secondary)
            }

            if let errorMessage {
                Text(errorMessage)
                    .inlineErrorText()
            }
        } header: {
            Text("BYOK Vault")
        } footer: {
            Text("Approve OpenRouter, ElevenLabs, AssemblyAI, Ollama Cloud, and Perplexity together. BYOK returns only the keys you choose to share.")
        }
    }

    private var connectedCount: Int {
        PodcastBYOKCredentialImporter.providers.filter {
            PodcastBYOKCredentialImporter.hasStoredKey(for: $0)
        }.count
    }

    private var statusTitle: String {
        if connectedCount == 0 { return "No provider keys connected" }
        if connectedCount == PodcastBYOKCredentialImporter.providers.count { return "All provider keys connected" }
        return "\(connectedCount) of \(PodcastBYOKCredentialImporter.providers.count) provider keys connected"
    }

    private func connectWithBYOK() async {
        errorMessage = nil
        message = nil
        isConnecting = true
        defer { isConnecting = false }

        do {
            let tokens = try await byokConnect.connectPodcastProviders()
            var settings = store.state.settings
            let imported = try PodcastBYOKCredentialImporter.apply(tokens, to: &settings)
            guard !imported.isEmpty else {
                throw BYOKConnectError.noProviderKeysReturned
            }
            store.updateSettings(settings)
            message = "Connected \(imported.map(\.provider.displayName).joined(separator: ", ")) with BYOK."
            Haptics.success()
        } catch BYOKConnectError.cancelled {
            Haptics.warning()
        } catch {
            errorMessage = error.localizedDescription
            Haptics.error()
        }
    }
}
