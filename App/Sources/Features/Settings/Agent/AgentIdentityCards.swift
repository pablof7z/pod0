import SwiftUI

// MARK: - Shared display constants

/// Display constants shared across Agent peer-management views.
enum NostrPubkeyDisplay {
    /// Number of hex characters shown in a truncated pubkey preview.
    static let prefixLength = 16
}

// MARK: - Layout constants

private enum AgentIdentityCardsLayout {
    /// Spacing between items inside the relay-card and key-management VStacks.
    static let cardItemSpacing: CGFloat = 12
    /// Uniform inset applied to text fields inside relay-card / key-management cards.
    static let fieldPadding: CGFloat = 10
    /// Uniform inset applied to the URL text field in the picture-URL sheet.
    static let urlFieldPadding: CGFloat = 12
    /// Spacing between items in the picture-URL sheet's main VStack.
    static let urlSheetSpacing: CGFloat = 20
    /// Spacing between Clear and Done buttons in the picture-URL sheet.
    static let urlSheetButtonSpacing: CGFloat = 12
}

// MARK: - Relay Card

struct AgentRelayCard: View {
    @Binding var relayURL: String

    var body: some View {
        VStack(alignment: .leading, spacing: AgentIdentityCardsLayout.cardItemSpacing) {
            HStack {
                Image(systemName: "antenna.radiowaves.left.and.right")
                    .font(AppTheme.Typography.title3)
                    .foregroundStyle(Color.accentColor)
                    .accessibilityHidden(true)
                Text("Relay")
                    .font(AppTheme.Typography.headline)
                Spacer()
            }

            TextField("wss://relay.damus.io", text: $relayURL)
                .font(AppTheme.Typography.monoCallout)
                .autocorrectionDisabled()
                .textInputAutocapitalization(.never)
                .keyboardType(.URL)
                .padding(AgentIdentityCardsLayout.fieldPadding)
                .background(Color(.quaternarySystemFill), in: RoundedRectangle(cornerRadius: AppTheme.Corner.md))

            Text("Your agent connects here to send and receive Nostr messages.")
                .font(AppTheme.Typography.caption2)
                .foregroundStyle(.tertiary)
        }
        .padding(AppTheme.Spacing.md)
        .glassSurface(cornerRadius: AppTheme.Corner.xl)
    }
}

// MARK: - Key Management Card

struct AgentKeyManagementCard: View {
    let hasPrivateKey: Bool
    let showCopied: Bool
    let npubEmpty: Bool
    @Binding var isExpanded: Bool
    @Binding var showImportKey: Bool
    @Binding var importKeyInput: String
    let onCopyPublicKey: () -> Void
    let onRegenerate: () -> Void
    let onGenerate: () -> Void
    let onImport: () -> Void

    var body: some View {
        DisclosureGroup(isExpanded: $isExpanded) {
            VStack(spacing: AgentIdentityCardsLayout.cardItemSpacing) {
                if hasPrivateKey {
                    Button {
                        onCopyPublicKey()
                    } label: {
                        Label(
                            showCopied ? "Copied" : "Copy Public Key",
                            systemImage: showCopied ? "checkmark" : "doc.on.doc"
                        )
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    .disabled(npubEmpty)

                    Button(role: .destructive) {
                        onRegenerate()
                    } label: {
                        Label("Regenerate Key Pair", systemImage: "arrow.triangle.2.circlepath")
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                } else {
                    Button {
                        onGenerate()
                    } label: {
                        Label("Generate Key Pair", systemImage: "key.fill")
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }

                importSection
            }
            .padding(.top, AppTheme.Spacing.sm)
        } label: {
            Label("Key Management", systemImage: "key.fill")
                .font(AppTheme.Typography.body)
        }
        .padding(AppTheme.Spacing.md)
        .glassSurface(cornerRadius: AppTheme.Corner.xl)
    }

    private var importSection: some View {
        DisclosureGroup("Import Private Key", isExpanded: $showImportKey) {
            VStack(spacing: AppTheme.Spacing.sm) {
                SecureField("Paste private key hex…", text: $importKeyInput)
                    .autocorrectionDisabled()
                    .textInputAutocapitalization(.never)
                    .font(AppTheme.Typography.monoCallout)
                    .padding(AgentIdentityCardsLayout.fieldPadding)
                    .background(Color(.quaternarySystemFill), in: RoundedRectangle(cornerRadius: AppTheme.Corner.md))

                Button("Import") {
                    onImport()
                }
                .disabled(importKeyInput.isBlank)
                .frame(maxWidth: .infinity)
            }
            .padding(.top, AppTheme.Spacing.sm)
        }
    }
}

// MARK: - Picture URL Sheet

struct AgentPictureURLSheet: View {
    @Binding var pictureURL: String
    @Binding var isPresented: Bool

    private var validPictureURL: URL? {
        let trimmed = pictureURL.trimmed
        guard !trimmed.isEmpty,
              let url = URL(string: trimmed),
              let scheme = url.scheme?.lowercased(),
              scheme == "http" || scheme == "https"
        else { return nil }
        return url
    }

    var body: some View {
        NavigationStack {
            VStack(spacing: AgentIdentityCardsLayout.urlSheetSpacing) {
                avatarPreview
                    .padding(.top, AppTheme.Spacing.sm)

                TextField("https://…", text: $pictureURL)
                    .keyboardType(.URL)
                    .autocorrectionDisabled()
                    .textInputAutocapitalization(.never)
                    .padding(AgentIdentityCardsLayout.urlFieldPadding)
                    .cardSurface()
                    .padding(.horizontal)

                HStack(spacing: AgentIdentityCardsLayout.urlSheetButtonSpacing) {
                    Button("Clear") {
                        pictureURL = ""
                        isPresented = false
                    }
                    .foregroundStyle(AppTheme.Tint.error)
                    .frame(maxWidth: .infinity)

                    Button("Done") {
                        isPresented = false
                    }
                    .buttonStyle(.borderedProminent)
                    .frame(maxWidth: .infinity)
                }
                .padding(.horizontal)

                Spacer()
            }
            .navigationTitle("Profile Picture")
            .navigationBarTitleDisplayMode(.inline)
        }
        .presentationDetents([.fraction(0.35)])
    }

    @ViewBuilder
    private var avatarPreview: some View {
        if let url = validPictureURL {
            CachedAsyncImage(url: url) { phase in
                switch phase {
                case .success(let image):
                    image.resizable().scaledToFill()
                default:
                    placeholderCircle
                }
            }
            .frame(width: AppTheme.Layout.iconLg, height: AppTheme.Layout.iconLg)
            .clipShape(Circle())
        } else {
            placeholderCircle
                .frame(width: AppTheme.Layout.iconLg, height: AppTheme.Layout.iconLg)
        }
    }

    private var placeholderCircle: some View {
        Circle()
            .fill(AppTheme.Tint.placeholder)
            .overlay(
                Image(systemName: "person.fill")
                    .foregroundStyle(.secondary)
                    .accessibilityHidden(true)
            )
    }
}
