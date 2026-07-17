import SwiftUI

// MARK: - ProviderConnectionConfig

/// Provider-specific bits behind the shared connection state machine.
/// Every provider screen used to reimplement the same BYOK / manual-key /
/// validate / disconnect flow; `ProviderConnectionSection` owns that flow
/// once and each screen supplies one of these.
struct ProviderConnectionConfig {
    /// Display name used in status strings and flash messages ("OpenRouter").
    let providerName: String
    let keyPlaceholder: String
    let footer: String
    /// Reads whether a key is currently stored in Keychain.
    let hasKey: () -> Bool
    let saveKey: (String) throws -> Void
    let deleteKey: () throws -> Void
    /// Runs the BYOK web-consent flow and returns the minted token.
    let connectBYOK: (BYOKConnectService) async throws -> BYOKTokenResponse
    /// How the stored key was obtained, for providers that track it in
    /// `Settings`. Nil for keychain-presence-only providers, which get a
    /// plain Connected / Not connected status row.
    var keySource: (() -> ProviderKeySource)?
    /// Settings bookkeeping after each transition; nil for keychain-only
    /// providers.
    var didConnectBYOK: ((BYOKTokenResponse) -> Void)?
    var didSaveManual: (() -> Void)?
    var didDisconnect: (() -> Void)?
    var validation: ProviderValidation?

    init(
        providerName: String,
        keyPlaceholder: String,
        footer: String,
        hasKey: @escaping () -> Bool,
        saveKey: @escaping (String) throws -> Void,
        deleteKey: @escaping () throws -> Void,
        connectBYOK: @escaping (BYOKConnectService) async throws -> BYOKTokenResponse,
        keySource: (() -> ProviderKeySource)? = nil,
        didConnectBYOK: ((BYOKTokenResponse) -> Void)? = nil,
        didSaveManual: (() -> Void)? = nil,
        didDisconnect: (() -> Void)? = nil,
        validation: ProviderValidation? = nil
    ) {
        self.providerName = providerName
        self.keyPlaceholder = keyPlaceholder
        self.footer = footer
        self.hasKey = hasKey
        self.saveKey = saveKey
        self.deleteKey = deleteKey
        self.connectBYOK = connectBYOK
        self.keySource = keySource
        self.didConnectBYOK = didConnectBYOK
        self.didSaveManual = didSaveManual
        self.didDisconnect = didDisconnect
        self.validation = validation
    }
}

enum ProviderKeySource {
    case none, byok, manual
}

/// Optional post-connect validation ("Validate Key", "Check Available
/// Models"). `run` hits the provider with the stored key and returns what
/// to render on success — a caption line, a card view, or both.
struct ProviderValidation {
    let idleLabel: String
    let busyLabel: String
    let icon: String
    let run: () async throws -> ProviderValidationResult
}

struct ProviderValidationResult {
    var caption: String?
    var card: AnyView?

    init(caption: String? = nil, card: AnyView? = nil) {
        self.caption = caption
        self.card = card
    }
}

// MARK: - ProviderConnectionSection

/// The shared "Connection" form section: status row, BYOK button, manual
/// key entry, optional validation, disconnect, and flash messages. Embed
/// inside a `Form`; provider screens keep only their extra sections (hero
/// cards, endpoints) around it.
struct ProviderConnectionSection: View {
    let config: ProviderConnectionConfig

    @State private var manualAPIKey = ""
    @State private var hasStoredKey = false
    @State private var isConnectingBYOK = false
    @State private var isValidating = false
    @State private var credentialMessage: String?
    @State private var credentialError: String?
    @State private var validationResult: ProviderValidationResult?
    @State private var byokConnect = BYOKConnectService()

    var body: some View {
        Section {
            Label(statusTitle, systemImage: statusIcon)
                .foregroundStyle(statusColor)

            byokButton

            RevealableAPIKeyField(config.keyPlaceholder, text: $manualAPIKey)
                .onSubmit { saveManualKey() }

            if !manualAPIKey.isBlank {
                Button {
                    saveManualKey()
                } label: {
                    Label("Save Key", systemImage: "square.and.arrow.down")
                }
            }

            if hasStoredKey, let validation = config.validation {
                validateButton(validation)
            }

            if hasStoredKey {
                Button(role: .destructive) {
                    disconnect()
                } label: {
                    Label("Disconnect", systemImage: "trash")
                }
            }

            if let card = validationResult?.card {
                card
                    .listRowInsets(AppTheme.Layout.cardRowInsetsXS)
                    .listRowBackground(Color.clear)
                    .transition(.opacity.combined(with: .move(edge: .top)))
            }

            if let caption = validationResult?.caption {
                Text(caption)
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.secondary)
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
            Text(config.footer)
        }
        .onAppear(perform: refreshCredentialState)
        .animation(AppTheme.Animation.spring, value: credentialMessage)
        .animation(AppTheme.Animation.spring, value: credentialError)
        .animation(AppTheme.Animation.spring, value: isConnectingBYOK)
    }

    // MARK: Rows

    private var byokButton: some View {
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
        .disabled(isConnectingBYOK)
    }

    private func validateButton(_ validation: ProviderValidation) -> some View {
        Button {
            Task { await runValidation(validation) }
        } label: {
            HStack {
                Label(
                    isValidating ? validation.busyLabel : validation.idleLabel,
                    systemImage: validation.icon
                )
                if isValidating {
                    Spacer()
                    ProgressView()
                }
            }
        }
        .disabled(isValidating)
    }

    // MARK: Status

    private var statusTitle: String {
        guard let keySource = config.keySource else {
            return hasStoredKey ? "Connected" : "Not connected"
        }
        guard hasStoredKey else {
            return keySource() == .none ? "Not connected" : "Reconnect required"
        }
        switch keySource() {
        case .byok:   return "Connected with BYOK"
        case .manual: return "Manual key saved"
        case .none:   return "Key stored"
        }
    }

    private var statusIcon: String {
        hasStoredKey ? "checkmark.seal.fill" : "xmark.seal"
    }

    private var statusColor: Color {
        hasStoredKey ? .green : .secondary
    }

    private var byokButtonTitle: String {
        config.keySource?() == .byok ? "Reconnect BYOK" : "Connect with BYOK"
    }

    // MARK: Actions

    private func connectWithBYOK() async {
        clearFlash()
        isConnectingBYOK = true
        defer { isConnectingBYOK = false }

        do {
            let token = try await config.connectBYOK(byokConnect)
            try config.saveKey(token.apiKey)
            config.didConnectBYOK?(token)
            manualAPIKey = ""
            refreshCredentialState()
            credentialMessage = "\(config.providerName) connected with BYOK."
            Haptics.success()
        } catch BYOKConnectError.cancelled {
            Haptics.warning()
        } catch {
            credentialError = error.localizedDescription
            Haptics.error()
        }
    }

    private func saveManualKey() {
        guard !manualAPIKey.isBlank else { return }
        clearFlash()
        do {
            try config.saveKey(manualAPIKey)
            config.didSaveManual?()
            manualAPIKey = ""
            refreshCredentialState()
            credentialMessage = "\(config.providerName) key saved in Keychain."
            Haptics.success()
        } catch {
            credentialError = "\(config.providerName) key could not be saved."
            Haptics.error()
        }
    }

    private func disconnect() {
        clearFlash()
        do {
            try config.deleteKey()
            config.didDisconnect?()
            manualAPIKey = ""
            refreshCredentialState()
            credentialMessage = "\(config.providerName) disconnected."
            Haptics.success()
        } catch {
            credentialError = "\(config.providerName) key could not be deleted."
            Haptics.error()
        }
    }

    private func runValidation(_ validation: ProviderValidation) async {
        clearFlash()
        isValidating = true
        defer { isValidating = false }

        do {
            validationResult = try await validation.run()
            Haptics.success()
        } catch {
            credentialError = error.localizedDescription
            Haptics.warning()
        }
    }

    private func clearFlash() {
        credentialMessage = nil
        credentialError = nil
        validationResult = nil
    }

    private func refreshCredentialState() {
        hasStoredKey = config.hasKey()
        if !hasStoredKey { validationResult = nil }
    }
}
