import SwiftUI

/// The "Connection" section of ElevenLabsSettingsView.
///
/// Extracted to keep `ElevenLabsSettingsView` under the 300-line soft limit.
/// All mutable state is owned by the parent and passed in via bindings and closures.
struct ElevenLabsConnectionSection: View {
    // MARK: - State from parent

    let statusTitle: String
    let statusIcon: String
    let byokButtonTitle: String
    let isConnectingBYOK: Bool
    let isValidatingKey: Bool
    let hasStoredKey: Bool
    let keyInfo: ElevenLabsKeyInfo?
    let credentialMessage: String?
    let credentialError: String?

    @Binding var manualAPIKey: String

    // MARK: - Actions

    let onConnectBYOK: () -> Void
    let onDisconnect: () -> Void
    let onValidateKey: () -> Void

    // MARK: - Body

    var body: some View {
        Section {
            Label(statusTitle, systemImage: statusIcon)

            Button(action: onConnectBYOK) {
                Label(isConnectingBYOK ? "Connecting..." : byokButtonTitle, systemImage: "key.viewfinder")
            }
            .buttonStyle(.glassProminent)
            .tint(AppTheme.Brand.elevenLabsTint)
            .disabled(isConnectingBYOK)

            apiKeyField

            if hasStoredKey {
                Button(role: .destructive, action: onDisconnect) {
                    Label("Disconnect ElevenLabs", systemImage: "trash")
                }
            }

            if hasStoredKey {
                Button(action: onValidateKey) {
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
                .tint(AppTheme.Brand.elevenLabsTint)
            }

            if let keyInfo {
                ElevenLabsKeyInfoCard(info: keyInfo)
                    .listRowInsets(AppTheme.Layout.cardRowInsetsXS)
                    .listRowBackground(Color.clear)
                    .transition(.opacity.combined(with: .move(edge: .top)))
            }

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
            Text("BYOK opens byok.f7z.io for consent and stores the returned ElevenLabs key in Keychain. Manual keys are also saved only in Keychain.")
        }
        .animation(AppTheme.Animation.spring, value: credentialMessage)
        .animation(AppTheme.Animation.spring, value: credentialError)
        .animation(AppTheme.Animation.spring, value: keyInfo?.tier)
    }

    // MARK: - Subviews

    private var apiKeyField: some View {
        RevealableAPIKeyField("Paste ElevenLabs API Key", text: $manualAPIKey)
    }
}
