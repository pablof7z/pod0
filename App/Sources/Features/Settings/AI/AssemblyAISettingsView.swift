import SwiftUI

struct AssemblyAISettingsView: View {
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
        .navigationTitle("AssemblyAI")
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

            RevealableAPIKeyField("Paste AssemblyAI API key", text: $manualAPIKey)
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
            Text("AssemblyAI powers cloud speech-to-text. Choose it for transcription in Models -> Speech.")
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
            let token = try await byokConnect.connectAssemblyAI()
            try AssemblyAICredentialStore.saveAPIKey(token.apiKey)
            manualAPIKey = ""
            refreshCredentialState()
            credentialMessage = "AssemblyAI connected with BYOK."
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
            try AssemblyAICredentialStore.saveAPIKey(manualAPIKey)
            manualAPIKey = ""
            refreshCredentialState()
            credentialMessage = "AssemblyAI key saved in Keychain."
            Haptics.success()
        } catch {
            credentialError = "AssemblyAI key could not be saved."
            Haptics.error()
        }
    }

    private func disconnect() {
        credentialError = nil
        credentialMessage = nil
        do {
            try AssemblyAICredentialStore.deleteAPIKey()
            manualAPIKey = ""
            refreshCredentialState()
            credentialMessage = "AssemblyAI disconnected."
            Haptics.success()
        } catch {
            credentialError = "AssemblyAI key could not be deleted."
            Haptics.error()
        }
    }

    private func refreshCredentialState() {
        hasStoredKey = AssemblyAICredentialStore.hasAPIKey()
    }
}
