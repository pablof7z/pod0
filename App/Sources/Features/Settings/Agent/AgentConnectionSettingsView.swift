import SwiftUI

struct AgentConnectionSettingsView: View {
    @Binding var relayURL: String
    let hasPrivateKey: Bool

    @Environment(\.dismiss) private var dismiss
    @State private var nsecText: String = ""
    @State private var nsecRevealed: Bool = false
    @State private var nsecCopied: Bool = false

    var body: some View {
        NavigationStack {
            Form {
                relaySection
                if hasPrivateKey {
                    privateKeySection
                }
            }
            .navigationTitle("Connection")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
        }
        .presentationDetents([.medium, .large])
    }

    // MARK: - Sections

    private var relaySection: some View {
        Section {
            TextField("wss://relay.damus.io", text: $relayURL)
                .font(AppTheme.Typography.monoCallout)
                .autocorrectionDisabled()
                .textInputAutocapitalization(.never)
                .keyboardType(.URL)
        } header: {
            Label("Relay", systemImage: "antenna.radiowaves.left.and.right")
        } footer: {
            Text("Your agent connects here to send and receive Nostr messages.")
        }
    }

    private var privateKeySection: some View {
        Section {
            if nsecRevealed && !nsecText.isEmpty {
                Text(nsecText)
                    .font(AppTheme.Typography.monoCaption)
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
                    .transition(.opacity)

                Button {
                    copyToClipboard(nsecText, isCopied: $nsecCopied)
                } label: {
                    Label(nsecCopied ? "Copied!" : "Copy nsec", systemImage: nsecCopied ? "checkmark" : "doc.on.doc")
                        .foregroundStyle(nsecCopied ? .green : .accentColor)
                }
            } else {
                Button {
                    loadNsec()
                    withAnimation(AppTheme.Animation.spring) { nsecRevealed = true }
                    Haptics.selection()
                } label: {
                    Label("Reveal private key", systemImage: "eye")
                }
            }
        } header: {
            Label("Private Key", systemImage: "key.fill")
        } footer: {
            Text("Your nsec is the private key that controls your Nostr identity. Never share it.")
        }
    }

    private func loadNsec() {
        guard let hex = (try? NostrCredentialStore.privateKey()) ?? nil,
              let data = Data(hexString: hex)
        else { return }
        nsecText = Bech32.encode(hrp: "nsec", data: data)
    }
}
