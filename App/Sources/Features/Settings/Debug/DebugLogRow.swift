import SwiftUI

// MARK: - DebugLogRow
//
// One row in `DebugLogsView`. Extracted from the list view to keep each file
// well under the length limit and to isolate the per-row layout: timestamp,
// color-coded level badge, category chip, and message body.

struct DebugLogRow: View {
    let entry: DiagnosticEntry

    var body: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
            HStack(spacing: AppTheme.Spacing.sm) {
                Text(Self.timeFormatter.string(from: entry.timestamp))
                    .font(AppTheme.Typography.mono)
                    .foregroundStyle(.secondary)

                levelBadge

                categoryChip

                Spacer(minLength: 0)
            }

            Text(entry.message)
                .font(AppTheme.Typography.monoCaption)
                .foregroundStyle(.primary)
                .textSelection(.enabled)
                .fixedSize(horizontal: false, vertical: true)
        }
        .padding(.vertical, 2)
    }

    private var levelBadge: some View {
        Text(entry.level.label)
            .font(AppTheme.Typography.mono)
            .foregroundStyle(.white)
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .background(entry.level.tint, in: RoundedRectangle(cornerRadius: 4))
    }

    private var categoryChip: some View {
        Text(entry.category)
            .font(AppTheme.Typography.mono)
            .foregroundStyle(.secondary)
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .background(AppTheme.Tint.surfaceMuted, in: RoundedRectangle(cornerRadius: 4))
    }

    /// `HH:mm:ss.SSS` — millisecond precision so adjacent ticks are
    /// distinguishable. Local to the row so the model stays UI-agnostic.
    private static let timeFormatter: DateFormatter = {
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.dateFormat = "HH:mm:ss.SSS"
        return f
    }()
}
