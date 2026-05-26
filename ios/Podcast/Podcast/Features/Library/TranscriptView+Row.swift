import SwiftUI

// MARK: - TranscriptRowView
//
// Renders one `TranscriptEntry` inside the `TranscriptView` list. Tapping
// the row dispatches a seek; the active row (highlighted by the parent)
// renders with the accent tint so the user can spot it while playback
// advances.

struct TranscriptRowView: View {
    let entry: TranscriptEntry
    let isActive: Bool
    let onTap: () -> Void

    var body: some View {
        Button(action: onTap) {
            HStack(alignment: .top, spacing: AppTheme.Spacing.md) {
                timestamp
                VStack(alignment: .leading, spacing: 2) {
                    if let speaker = entry.speaker, !speaker.isEmpty {
                        Text(speaker)
                            .font(AppTheme.Typography.caption)
                            .foregroundStyle(.secondary)
                    }
                    Text(entry.text)
                        .font(AppTheme.Typography.body)
                        .foregroundStyle(isActive ? AnyShapeStyle(Color.accentColor) : AnyShapeStyle(.primary))
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .multilineTextAlignment(.leading)
                }
            }
            .padding(.vertical, AppTheme.Spacing.xs)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(accessibilityLabel)
        .accessibilityAddTraits(.isButton)
    }

    // MARK: - Subviews

    private var timestamp: some View {
        Text(formatDuration(entry.startSecs))
            .font(AppTheme.Typography.caption.monospacedDigit())
            .foregroundStyle(isActive ? AnyShapeStyle(Color.accentColor) : AnyShapeStyle(.tertiary))
            .frame(minWidth: 44, alignment: .leading)
    }

    private var accessibilityLabel: String {
        let prefix = entry.speaker.map { "\($0) at " } ?? "At "
        return "\(prefix)\(formatDuration(entry.startSecs)): \(entry.text)"
    }

}
