import SwiftUI

struct AgentIdentityHero: View {
    @Binding var settings: Settings
    let hasPrivateKey: Bool
    let npubFull: String
    var nameFocused: FocusState<Bool>.Binding
    var bioFocused: FocusState<Bool>.Binding
    let onEditPicture: () -> Void
    let onShowQR: () -> Void

    private var displayName: String {
        let name = settings.nostrProfileName.trimmed
        return name.isEmpty ? "Agent" : name
    }

    var body: some View {
        VStack(spacing: AppTheme.Spacing.md) {
            avatarButton

            VStack(spacing: AppTheme.Spacing.sm) {
                TextField("Agent name", text: $settings.nostrProfileName)
                    .font(AppTheme.Typography.title)
                    .multilineTextAlignment(.center)
                    .focused(nameFocused)

                TextField("Short bio", text: $settings.nostrProfileAbout, axis: .vertical)
                    .font(AppTheme.Typography.callout)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .lineLimit(2...4)
                    .focused(bioFocused)
            }
            .textFieldStyle(.plain)
            .padding(.horizontal, AppTheme.Spacing.lg)

            if hasPrivateKey {
                Button {
                    onShowQR()
                } label: {
                    Label(npubFull.isEmpty ? "Show Public Key" : npubFull, systemImage: "qrcode")
                        .font(AppTheme.Typography.monoCaption)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
                .buttonStyle(.glass)
                .disabled(npubFull.isEmpty)
            }
        }
        .frame(maxWidth: .infinity)
        .padding(.horizontal, AppTheme.Spacing.lg)
    }

    private var avatarButton: some View {
        Button {
            onEditPicture()
        } label: {
            ZStack {
                Circle()
                    .fill(AppTheme.Tint.agentSurface.opacity(0.16))

                if let url = avatarURL {
                    CachedAsyncImage(url: url) { phase in
                        if case .success(let image) = phase {
                            image.resizable().scaledToFill()
                        } else {
                            initials
                        }
                    }
                    .clipShape(Circle())
                } else {
                    initials
                }
            }
            .frame(width: 104, height: 104)
            .overlay(alignment: .bottomTrailing) {
                Image(systemName: "camera.fill")
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.white)
                    .padding(8)
                    .background(Color.accentColor, in: Circle())
            }
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Edit agent picture")
    }

    private var initials: some View {
        Text(String(displayName.prefix(1)).uppercased())
            .font(.system(size: 42, weight: .bold, design: .rounded))
            .foregroundStyle(AppTheme.Tint.agentSurface)
    }

    private var avatarURL: URL? {
        guard let url = URL(string: settings.nostrProfilePicture),
              let scheme = url.scheme?.lowercased(),
              scheme == "http" || scheme == "https" else { return nil }
        return url
    }
}
