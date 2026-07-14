import SwiftUI

struct AgentIdentityView: View {

    private enum Layout {
        static let heroGradientHeight: CGFloat = 340
        static let cardPadding: CGFloat = 16
        static let generateButtonPaddingV: CGFloat = 12
        static let generateCardSpacing: CGFloat = 12
        static let shareCardSpacing: CGFloat = 10
    }

    @Environment(AppStateStore.self) private var store

    @State private var settings: Settings = Settings()
    @State private var hasPrivateKey: Bool = false
    @State private var showCopied: Bool = false
    @State private var showRegenerateConfirm: Bool = false
    @State private var importKeyInput: String = ""
    @State private var showImportKey: Bool = false
    @State private var showQRFullScreen: Bool = false
    @State private var showConnectionSettings: Bool = false
    @State private var editingPictureURL: Bool = false
    @State private var keyManagementExpanded: Bool = false
    @State private var keychainErrorMessage: String?
    @FocusState private var nameFocused: Bool
    @FocusState private var bioFocused: Bool

    var body: some View {
        ScrollView {
            ZStack(alignment: .top) {
                LinearGradient(
                    colors: [Color.accentColor.opacity(0.18), Color.clear],
                    startPoint: .top, endPoint: .bottom
                )
                .frame(height: Layout.heroGradientHeight)
                .ignoresSafeArea(edges: .top)
                .allowsHitTesting(false)

                VStack(spacing: 0) {
                    AgentIdentityHero(
                        settings: $settings,
                        hasPrivateKey: hasPrivateKey,
                        npubFull: npubFull,
                        nameFocused: $nameFocused,
                        bioFocused: $bioFocused,
                        onEditPicture: { editingPictureURL = true },
                        onShowQR: { showQRFullScreen = true }
                    )
                    .padding(.top, AppTheme.Spacing.lg)

                    cardsSection.padding(.top, AppTheme.Spacing.md)
                    footerNote
                }
            }
        }
        .navigationTitle("Identity")
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.hidden, for: .navigationBar)
        .toolbar {
            ToolbarItem(placement: .navigationBarTrailing) {
                HStack(spacing: AppTheme.Spacing.xs) {
                    Button { showConnectionSettings = true } label: {
                        Image(systemName: "gear")
                    }
                    .accessibilityLabel("Connection settings")

                    Button { showQRFullScreen = true } label: {
                        Image(systemName: "qrcode")
                    }
                    .accessibilityLabel("Show QR code")
                    .disabled(!hasPrivateKey)

                    Button {
                        // `SystemShareSheet.present` rather than a SwiftUI
                        // `.sheet { ShareSheet(items:) }` because the items
                        // array contains a deep-link `URL`, and the SwiftUI
                        // sheet wrapper renders blank for any items
                        // `UIActivityViewController` treats as files /
                        // URL-handlers on iOS 16+.
                        SystemShareSheet.present(items: shareInviteItems)
                    } label: {
                        Image(systemName: "square.and.arrow.up")
                    }
                    .accessibilityLabel("Share my identity")
                    .disabled(!hasPrivateKey || npubFull.isEmpty)
                }
            }
        }
        .onAppear {
            settings = store.state.settings
            refreshKeyState()
            keyManagementExpanded = !hasPrivateKey
        }
        .onChange(of: settings) { _, new in store.updateSettings(new) }
        .alert("Regenerate Key Pair?", isPresented: $showRegenerateConfirm) {
            Button("Regenerate", role: .destructive) { generateKeyPair(); Haptics.success() }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("This permanently replaces your current Nostr identity. Friends who know your old key will no longer recognize you.")
        }
        .fullScreenCover(isPresented: $showQRFullScreen) {
            AgentIdentityQRView(npub: npubFull, name: settings.nostrProfileName)
                .presentationBackground(.clear)
        }
        .sheet(isPresented: $showConnectionSettings) {
            AgentConnectionSettingsView(
                relayURL: $settings.nostrRelayURL,
                hasPrivateKey: hasPrivateKey
            )
        }
        .sheet(isPresented: $editingPictureURL) {
            AgentPictureURLSheet(pictureURL: $settings.nostrProfilePicture, isPresented: $editingPictureURL)
        }
        .alert(
            "Couldn't save key",
            isPresented: Binding(
                get: { keychainErrorMessage != nil },
                set: { if !$0 { keychainErrorMessage = nil } }
            ),
            presenting: keychainErrorMessage
        ) { _ in
            Button("OK", role: .cancel) { keychainErrorMessage = nil }
        } message: { msg in
            Text(msg)
        }
    }

    // MARK: - Cards section

    private var cardsSection: some View {
        GlassEffectContainer(spacing: Layout.cardPadding) {
            if !hasPrivateKey {
                generateKeyCard
            }
            if hasPrivateKey && !npubFull.isEmpty {
                shareInviteCard
            }
            AgentKeyManagementCard(
                hasPrivateKey: hasPrivateKey,
                showCopied: showCopied,
                npubEmpty: npubFull.isEmpty,
                isExpanded: $keyManagementExpanded,
                showImportKey: $showImportKey,
                importKeyInput: $importKeyInput,
                onCopyPublicKey: copyPublicKey,
                onRegenerate: { showRegenerateConfirm = true },
                onGenerate: { generateKeyPair(); Haptics.success() },
                onImport: importPrivateKey
            )
        }
        .padding(.horizontal, Layout.cardPadding)
        .padding(.bottom, AppTheme.Spacing.sm)
    }

    private var shareInviteCard: some View {
        VStack(spacing: Layout.shareCardSpacing) {
            HStack {
                Image(systemName: "person.badge.plus")
                    .font(AppTheme.Typography.title3)
                    .foregroundStyle(Color.accentColor)
                    .accessibilityHidden(true)
                Text("Invite a Friend")
                    .font(AppTheme.Typography.headline)
                Spacer()
            }
            Text("Share your public key so friends can send you clips, episodes, and agent prompts over Nostr.")
                .font(AppTheme.Typography.caption)
                .foregroundStyle(.secondary)
            Button {
                Haptics.selection()
                SystemShareSheet.present(items: shareInviteItems)
            } label: {
                Label("Share My Identity", systemImage: "square.and.arrow.up")
                    .font(.system(.body, design: .rounded, weight: .semibold))
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, Layout.generateButtonPaddingV)
            }
            .buttonStyle(.borderedProminent)
        }
        .padding(AppTheme.Spacing.md)
        .glassSurface(cornerRadius: AppTheme.Corner.xl)
    }

    private var generateKeyCard: some View {
        VStack(spacing: Layout.generateCardSpacing) {
            Text("No identity yet")
                .font(AppTheme.Typography.headline)
            Text("Generate a key pair to create your Nostr identity.")
                .font(AppTheme.Typography.callout).foregroundStyle(.secondary).multilineTextAlignment(.center)
            Button { generateKeyPair(); Haptics.success() } label: {
                Label("Generate Key Pair", systemImage: "key.fill")
                    .font(.system(.body, design: .rounded, weight: .semibold))
                    .frame(maxWidth: .infinity).padding(.vertical, Layout.generateButtonPaddingV)
            }
            .buttonStyle(.borderedProminent)
        }
        .padding(AppTheme.Spacing.md)
        .glassSurface(cornerRadius: AppTheme.Corner.xl)
    }

    private var footerNote: some View {
        Text("Private key is stored in Keychain and never leaves this device.")
            .font(AppTheme.Typography.caption2).foregroundStyle(.tertiary)
            .multilineTextAlignment(.center)
            .padding(.horizontal, AppTheme.Spacing.xl).padding(.vertical, AppTheme.Spacing.md)
    }

    // MARK: - Computed

    private var npubFull: String {
        guard let hex = settings.nostrPublicKeyHex, !hex.isEmpty,
              let data = Data(hexString: hex)
        else { return "" }
        return Bech32.encode(hrp: "npub", data: data)
    }

    /// Items passed to the system share sheet when the user taps "Share My Identity".
    /// Includes a human-readable invite text and a deep-link URL the recipient can tap
    /// to open `AddFriendSheet` with the sender's details pre-filled.
    private var shareInviteItems: [Any] {
        let name = settings.nostrProfileName.trimmed
        let displayedName = name.isEmpty ? "a friend" : name
        let inviteURL = DeepLinkHandler.friendInviteURL(
            npub: npubFull,
            name: name.isEmpty ? nil : name
        )
        var items: [Any] = [
            "Add \(displayedName) on Podcastr: \(npubFull)"
        ]
        if let url = inviteURL { items.append(url) }
        return items
    }

    // MARK: - Actions

    private func generateKeyPair() {
        do {
            let pair = try NostrKeyPair.generate()
            try NostrCredentialStore.savePrivateKey(pair.privateKeyHex)
            settings.nostrPublicKeyHex = pair.publicKeyHex
            refreshKeyState()
        } catch {
            keychainErrorMessage = "Could not generate key pair: \(error.localizedDescription)"
            Haptics.error()
        }
    }

    private func importPrivateKey() {
        let trimmed = importKeyInput.trimmed.lowercased()
        guard !trimmed.isEmpty else { return }
        do {
            let pair: NostrKeyPair
            if trimmed.hasPrefix("nsec") {
                pair = try NostrKeyPair(nsec: trimmed)
            } else {
                pair = try NostrKeyPair(privateKeyHex: trimmed)
            }
            try NostrCredentialStore.savePrivateKey(pair.privateKeyHex)
            settings.nostrPublicKeyHex = pair.publicKeyHex
            importKeyInput = ""
            refreshKeyState()
            Haptics.success()
        } catch {
            keychainErrorMessage = "Invalid private key — paste the raw hex or nsec bech32."
            Haptics.error()
        }
    }

    private func copyPublicKey() {
        guard !npubFull.isEmpty else { return }
        copyToClipboard(npubFull, isCopied: $showCopied)
    }

    private func refreshKeyState() {
        hasPrivateKey = NostrCredentialStore.hasPrivateKey()
    }
}
