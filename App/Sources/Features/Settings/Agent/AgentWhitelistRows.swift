import SwiftUI
import UIKit

// MARK: - AllowedRow

struct AllowedRow: View {
    let key: String

    @State private var isCopied = false

    var body: some View {
        Button { copyToClipboard(key, isCopied: $isCopied) } label: {
            HStack(spacing: AppTheme.Spacing.sm) {
                Image(systemName: "checkmark.shield.fill")
                    .foregroundStyle(AppTheme.Tint.success)

                Text(NostrNpub.shortNpub(fromHex: key))
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
