import SwiftUI

// MARK: - CodeBlockView

/// Renders a fenced code block with a monospaced scrollable surface and a
/// one-tap copy button. Uses `.glassSurface` for visual consistency with
/// other glass surfaces in the chat.
struct CodeBlockView: View {

    let language: String?
    let code: String

    @State private var copied = false

    private enum Layout {
        static let innerPaddingH: CGFloat = AppTheme.Spacing.md
        static let innerPaddingV: CGFloat = AppTheme.Spacing.sm
        static let headerHeight: CGFloat = 28
        static let copyIconSize: CGFloat = 13
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            Divider()
                .opacity(0.3)
            codeBody
        }
        .background(
            Color(.secondarySystemBackground).opacity(0.6),
            in: RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous)
        )
        .overlay(
            RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous)
                .strokeBorder(Color.secondary.opacity(0.18), lineWidth: 0.5)
        )
    }

    // MARK: - Header bar

    private var header: some View {
        HStack(spacing: AppTheme.Spacing.xs) {
            if let language {
                Text(language.lowercased())
                    .font(AppTheme.Typography.mono)
                    .foregroundStyle(.secondary)
            } else {
                Text("code")
                    .font(AppTheme.Typography.mono)
                    .foregroundStyle(.tertiary)
            }

            Spacer(minLength: 0)

            Button {
                copyToClipboard(code, isCopied: $copied)
            } label: {
                HStack(spacing: AppTheme.Spacing.xs) {
                    Image(systemName: copied ? "checkmark" : "doc.on.doc")
                        .font(.system(size: Layout.copyIconSize, weight: .medium))
                    Text(copied ? "Copied!" : "Copy")
                        .font(AppTheme.Typography.caption)
                }
                .foregroundStyle(copied ? AppTheme.Tint.success : Color.secondary)
                .contentTransition(.symbolEffect(.replace))
            }
            .buttonStyle(.plain)
            .accessibilityLabel(copied ? "Copied to clipboard" : "Copy code")
        }
        .padding(.horizontal, Layout.innerPaddingH)
        .frame(height: Layout.headerHeight)
    }

    // MARK: - Code body

    private var codeBody: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            Text(code)
                .font(AppTheme.Typography.mono)
                .foregroundStyle(.primary)
                .textSelection(.enabled)
                .fixedSize(horizontal: true, vertical: false)
                .padding(.horizontal, Layout.innerPaddingH)
                .padding(.vertical, Layout.innerPaddingV)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
