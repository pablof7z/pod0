import SwiftUI

// MARK: - RemoteSignerView
//
// Per identity-05-synthesis §4.7. Promotes `Nip46ConnectCard` to a primary
// push surface — the card paints itself in `.primary` mode (no outer chrome,
// no header glyph, no footnote). This view supplies the title, the prose
// intro, and the trailing footnote about keys never touching the device.

struct RemoteSignerView: View {

    @Environment(UserIdentityStore.self) private var identity
    @State private var bunkerInput = ""
    @State private var isConnecting = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: AppTheme.Spacing.lg) {
                preface

                Nip46ConnectCard(
                    bunkerInput: $bunkerInput,
                    isConnectingRemote: $isConnecting,
                    connect: { await connect() },
                    disconnect: { await identity.disconnectRemoteSigner() },
                    presentation: .primary
                )

                footnote
            }
            .padding(AppTheme.Spacing.lg)
        }
        .navigationTitle("Remote signer")
        .navigationBarTitleDisplayMode(.inline)
        .background(Color(.systemBackground))
    }

    // MARK: - Sections

    private var preface: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
            Text("Some people prefer to keep their key in a separate bunker service and let other apps ask permission to post. Podcastr supports bunker links.")
                .font(AppTheme.Typography.body)
                .foregroundStyle(.primary)

            Text("Open your signer app, find \u{201C}connect a new app\u{201D} (it might say \u{201C}bunker\u{201D}), and paste the link here.")
                .font(AppTheme.Typography.body)
                .foregroundStyle(.secondary)
        }
    }

    private var footnote: some View {
        Text("Your private key never touches this device — every signature happens inside your signer app.")
            .font(AppTheme.Typography.caption)
            .foregroundStyle(.tertiary)
    }

    // MARK: - Connect

    private func connect() async {
        let trimmed = bunkerInput.trimmed
        guard !trimmed.isEmpty else { return }
        isConnecting = true
        await identity.connectRemoteSigner(uri: trimmed)
        isConnecting = false
        if identity.isRemoteSigner {
            bunkerInput = ""
            Haptics.success()
        } else {
            Haptics.error()
        }
    }
}
