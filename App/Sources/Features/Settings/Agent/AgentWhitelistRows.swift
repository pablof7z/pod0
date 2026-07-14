import SwiftUI
import UIKit

// MARK: - PendingApprovalRow

struct PendingApprovalRow: View {

    private enum Layout {
        static let pendingApprovalSubSpacing: CGFloat = 2
    }

    let approval: NostrPendingApproval
    let onAllow: () -> Void
    let onBlock: () -> Void
    let onDismiss: () -> Void

    @State private var pubkeyCopied = false

    var body: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
            HStack(alignment: .center, spacing: AppTheme.Spacing.sm) {
                ZStack {
                    Circle().fill(LinearGradient(
                        colors: [AppTheme.Tint.warning.opacity(0.3), AppTheme.Tint.warning.opacity(0.1)],
                        startPoint: .top,
                        endPoint: .bottom
                    ))
                    Image(systemName: "person.fill")
                        .foregroundStyle(AppTheme.Tint.warning)
                }
                .frame(width: AppTheme.Layout.iconSm, height: AppTheme.Layout.iconSm)

                VStack(alignment: .leading, spacing: Layout.pendingApprovalSubSpacing) {
                    Text(approval.displayName ?? "Unknown")
                        .font(AppTheme.Typography.headline)

                    Button {
                        copyToClipboard(approval.pubkeyHex, isCopied: $pubkeyCopied)
                    } label: {
                        HStack(spacing: AppTheme.Spacing.xs) {
                            Text(approval.shortPubkey)
                                .font(AppTheme.Typography.monoCaption)
                                .foregroundStyle(.secondary)
                            if pubkeyCopied {
                                Image(systemName: "checkmark")
                                    .font(AppTheme.Typography.caption2)
                                    .foregroundStyle(.secondary)
                                    .transition(.opacity.combined(with: .scale))
                            }
                        }
                        .animation(AppTheme.Animation.springFast, value: pubkeyCopied)
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel(pubkeyCopied ? "Copied" : "Copy public key")

                    Text(approval.receivedAt.formatted(date: .abbreviated, time: .shortened))
                        .font(AppTheme.Typography.caption)
                        .foregroundStyle(.tertiary)
                }

                Spacer(minLength: 0)
            }

            // Inline action buttons — swipe actions are also available
            HStack(spacing: AppTheme.Spacing.sm) {
                Button(action: onAllow) {
                    Label("Allow", systemImage: "checkmark.circle.fill")
                        .font(AppTheme.Typography.caption.weight(.semibold))
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
                .tint(AppTheme.Tint.success)

                Button(action: onBlock) {
                    Label("Block", systemImage: "nosign")
                        .font(AppTheme.Typography.caption.weight(.semibold))
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.bordered)
                .tint(AppTheme.Tint.error)

                Button(action: onDismiss) {
                    Label("Dismiss", systemImage: "xmark")
                        .font(AppTheme.Typography.caption.weight(.semibold))
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.bordered)
                .tint(.secondary)
            }
        }
        .padding(.vertical, AppTheme.Spacing.xs)
    }
}

// MARK: - AllowedRow

struct AllowedRow: View {
    let key: String

    @State private var isCopied = false

    var body: some View {
        Button { copyToClipboard(key, isCopied: $isCopied) } label: {
            HStack(spacing: AppTheme.Spacing.sm) {
                Image(systemName: "checkmark.shield.fill")
                    .foregroundStyle(AppTheme.Tint.success)

                Text("npub1\(key.prefix(NostrPubkeyDisplay.prefixLength))…")
                    .font(AppTheme.Typography.monoCallout)
                    .foregroundStyle(.primary)

                Spacer()

                if isCopied {
                    Label("Copied", systemImage: "checkmark")
                        .labelStyle(.titleAndIcon)
                        .font(AppTheme.Typography.caption)
                        .foregroundStyle(.secondary)
                        .transition(.opacity)
                }
            }
        }
        .buttonStyle(.plain)
        .contentShape(Rectangle())
        .accessibilityLabel(isCopied ? "Copied" : "Copy public key")
        .animation(AppTheme.Animation.easeOut, value: isCopied)
    }
}

// MARK: - AllowPeerSheet

struct AllowPeerSheet: View {
    @Environment(\.dismiss) private var dismiss
    @State private var hexInput: String = ""
    let onAllow: (String) -> Void

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField("Hex pubkey…", text: $hexInput)
                        .autocorrectionDisabled()
                        .textInputAutocapitalization(.never)
                        .font(AppTheme.Typography.monoCallout)
                } footer: {
                    Text("Paste a Nostr public key in hex format. The peer will be allowed to contact your agent.")
                }
            }
            .navigationTitle("Allow Peer")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Allow") {
                        let trimmed = hexInput.trimmed.lowercased()
                        guard !trimmed.isEmpty else { return }
                        onAllow(trimmed)
                        dismiss()
                    }
                    .fontWeight(.semibold)
                    .disabled(hexInput.isBlank)
                }
            }
        }
    }
}
