import SwiftUI

/// AssemblyAI connection screen — keychain-presence-only provider on the
/// shared `ProviderConnectionSection` state machine.
struct AssemblyAISettingsView: View {
    var body: some View {
        Form {
            ProviderConnectionSection(config: Self.config)
        }
        .listStyle(.insetGrouped)
        .navigationTitle("AssemblyAI")
        .navigationBarTitleDisplayMode(.inline)
    }

    private static let config = ProviderConnectionConfig(
        providerName: "AssemblyAI",
        keyPlaceholder: "Paste AssemblyAI API key",
        footer: "AssemblyAI powers cloud speech-to-text. Choose it for transcription in Models -> Speech.",
        hasKey: { AssemblyAICredentialStore.hasAPIKey() },
        saveKey: { try AssemblyAICredentialStore.saveAPIKey($0) },
        deleteKey: { try AssemblyAICredentialStore.deleteAPIKey() },
        connectBYOK: { try await $0.connectAssemblyAI() }
    )
}
