import SwiftUI

/// A compact capsule-shaped status badge used in feedback thread rows and
/// the thread-detail navigation bar to display the current thread status.
///
/// Uses `AppTheme.Spacing.sm`/`AppTheme.Spacing.xs` for horizontal/vertical
/// padding so both call sites stay visually consistent.
struct FeedbackStatusBadge: View {
    let status: String

    var body: some View {
        Text(status.uppercased())
            .font(AppTheme.Typography.caption2.weight(.semibold))
            .padding(.horizontal, AppTheme.Spacing.sm)
            .padding(.vertical, AppTheme.Spacing.xs)
            .background(Color.accentColor.opacity(0.15), in: .capsule)
            .foregroundStyle(Color.accentColor)
            .accessibilityLabel("Status: \(status)")
    }
}
