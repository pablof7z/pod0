import SwiftUI

/// Shared layout constants and helpers used by both `ElevenLabsKeyInfoCard`
/// and `OpenRouterKeyInfoCard`.  Values that map to an `AppTheme` token are
/// expressed via that token; values that fall between two tokens are kept as
/// named constants so the intent remains clear.
enum KeyInfoCardLayout {

    // MARK: - Card shell

    /// 14 pt — between Corner.md (12) and Corner.lg (16); unique to this card style.
    static let cardCornerRadius: CGFloat = 14
    /// 10 pt — internal VStack row gap.
    static let rowSpacing: CGFloat = 10

    // MARK: - Header / tier row

    /// 8 pt — horizontal HStack spacing. Matches AppTheme.Spacing.sm.
    static let hStackSpacing: CGFloat = AppTheme.Spacing.sm
    /// 16 pt — validated-checkmark icon point size.
    static let headerIconSize: CGFloat = 16

    // MARK: - Quota / credit bar

    /// 6 pt — progress bar fill height.
    static let barHeight: CGFloat = 6
    /// 6 pt — vertical spacing inside the quota VStack.
    static let quotaSpacing: CGFloat = 6

    // MARK: - Tier chip

    /// 8 pt — corner radius for tier chips. Matches AppTheme.Corner.sm.
    static let chipCornerRadius: CGFloat = AppTheme.Corner.sm
    /// 8 pt — horizontal chip padding. Matches AppTheme.Spacing.sm.
    static let chipHPadding: CGFloat = AppTheme.Spacing.sm
    /// 4 pt — vertical chip padding. Matches AppTheme.Spacing.xs.
    static let chipVPadding: CGFloat = AppTheme.Spacing.xs
    /// 4 pt — icon-to-label spacing inside chip. Matches AppTheme.Spacing.xs.
    static let chipInnerSpacing: CGFloat = AppTheme.Spacing.xs
    /// 10 pt — chip icon point size.
    static let chipIconSize: CGFloat = 10
    /// 11 pt — chip label point size.
    static let chipLabelSize: CGFloat = 11
    /// 0.12 — tint opacity for chip background fill.
    static let chipBackgroundOpacity: Double = 0.12
}

// MARK: - Bar color helper

/// Returns a traffic-light tint for a usage progress bar.
///
/// - Parameters:
///   - fraction: Remaining fraction in [0, 1].
///   - highColor: Color used when `fraction > 0.5` (provider-specific brand tint).
/// - Returns: `highColor` above 50 %, orange between 20–50 %, red below 20 %.
func keyInfoBarColor(fraction: Double, highColor: Color) -> Color {
    if fraction > 0.5 { return highColor }
    if fraction > 0.2 { return .orange }
    return .red
}

// MARK: - Tier chip

/// Compact pill badge showing a subscription tier label and an icon.
///
/// Used by both `ElevenLabsKeyInfoCard` and `OpenRouterKeyInfoCard`. The chip
/// switches between a "free" appearance (gift icon, orange tint) and a "paid"
/// appearance (credit-card icon, caller-supplied `paidColor`) based on
/// `isFreeTier`.
///
/// - Parameters:
///   - label: Human-readable tier label (e.g. "Free tier", "Starter").
///   - isFreeTier: When `true` the chip shows a gift icon with an orange tint.
///   - paidColor: Tint applied when `isFreeTier` is `false`.
struct KeyInfoTierChip: View {
    let label: String
    let isFreeTier: Bool
    let paidColor: Color

    private var tint: Color { isFreeTier ? .orange : paidColor }

    var body: some View {
        HStack(spacing: KeyInfoCardLayout.chipInnerSpacing) {
            Image(systemName: isFreeTier ? "gift" : "creditcard")
                .font(.system(size: KeyInfoCardLayout.chipIconSize, weight: .semibold))
                .accessibilityHidden(true)
            Text(label)
                .font(.system(size: KeyInfoCardLayout.chipLabelSize, weight: .medium))
        }
        .foregroundStyle(tint)
        .padding(.horizontal, KeyInfoCardLayout.chipHPadding)
        .padding(.vertical, KeyInfoCardLayout.chipVPadding)
        .background(
            tint.opacity(KeyInfoCardLayout.chipBackgroundOpacity),
            in: RoundedRectangle(cornerRadius: KeyInfoCardLayout.chipCornerRadius, style: .continuous)
        )
    }
}
