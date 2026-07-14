import SwiftUI

/// Compact card displayed after a successful ElevenLabs key validation.
/// Shows character quota and subscription tier.
struct ElevenLabsKeyInfoCard: View {

    let info: ElevenLabsKeyInfo

    // Layout constants live in KeyInfoCardShared.swift (shared with OpenRouterKeyInfoCard).
    private typealias Layout = KeyInfoCardLayout

    var body: some View {
        VStack(alignment: .leading, spacing: Layout.rowSpacing) {
            headerRow
            if info.remainingFraction != nil || info.remainingLabel != nil {
                quotaSection
            }
            if info.tier != nil {
                tierRow
            }
        }
        .padding(AppTheme.Spacing.md)
        .background(Color(.secondarySystemGroupedBackground))
        .clipShape(RoundedRectangle(cornerRadius: Layout.cardCornerRadius, style: .continuous))
        .accessibilityElement(children: .combine)
        .accessibilityLabel(accessibilityDescription)
    }

    // MARK: - Sub-views

    private var headerRow: some View {
        HStack(spacing: Layout.hStackSpacing) {
            Image(systemName: "checkmark.seal.fill")
                .foregroundStyle(AppTheme.Brand.elevenLabsTint)
                .font(.system(size: Layout.headerIconSize, weight: .semibold))
            Text("Key validated")
                .font(AppTheme.Typography.subheadline.weight(.semibold))
                .foregroundStyle(.primary)
            Spacer()
        }
    }

    @ViewBuilder
    private var quotaSection: some View {
        VStack(alignment: .leading, spacing: Layout.quotaSpacing) {
            if let label = info.remainingLabel {
                Text(label)
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.secondary)
            }

            if let fraction = info.remainingFraction {
                GeometryReader { geo in
                    ZStack(alignment: .leading) {
                        Capsule()
                            .fill(Color(.systemFill))
                            .frame(height: Layout.barHeight)
                        Capsule()
                            .fill(keyInfoBarColor(fraction: fraction, highColor: AppTheme.Brand.elevenLabsTint))
                            .frame(width: geo.size.width * fraction, height: Layout.barHeight)
                    }
                }
                .frame(height: Layout.barHeight)
            }
        }
    }

    private var tierRow: some View {
        HStack(spacing: Layout.hStackSpacing) {
            if let tier = info.tier {
                tierChip(tier: tier)
            }
            Spacer()
        }
    }

    private func tierChip(tier: String) -> some View {
        KeyInfoTierChip(
            label: tier.capitalized,
            isFreeTier: tier.lowercased() == "free",
            paidColor: AppTheme.Brand.elevenLabsTint
        )
    }

    // MARK: - Helpers

    private var accessibilityDescription: String {
        var parts = ["Key validated"]
        if let label = info.remainingLabel { parts.append(label) }
        if let tier = info.tier { parts.append("Plan: \(tier)") }
        return parts.joined(separator: ", ")
    }
}
