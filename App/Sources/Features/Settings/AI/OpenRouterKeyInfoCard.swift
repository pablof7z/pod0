import SwiftUI

/// Compact card displayed after a successful OpenRouter key validation.
/// Shows credit usage, rate-limit tier, and key label.
struct OpenRouterKeyInfoCard: View {

    let info: OpenRouterKeyInfo

    // Layout constants live in KeyInfoCardShared.swift (shared with ElevenLabsKeyInfoCard).
    private typealias Layout = KeyInfoCardLayout

    var body: some View {
        VStack(alignment: .leading, spacing: Layout.rowSpacing) {
            headerRow
            if info.remainingFraction != nil || info.limitDollars != nil {
                creditSection
            }
            tierRow
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
                .foregroundStyle(AppTheme.Tint.success)
                .font(.system(size: Layout.headerIconSize, weight: .semibold))
            Text("Key validated")
                .font(AppTheme.Typography.subheadline.weight(.semibold))
                .foregroundStyle(.primary)
            Spacer()
            if let label = info.label, !label.isEmpty {
                Text(label)
                    .font(AppTheme.Typography.monoCaption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
        }
    }

    @ViewBuilder
    private var creditSection: some View {
        VStack(alignment: .leading, spacing: Layout.quotaSpacing) {
            if let remaining = info.remainingLabel {
                Text(remaining)
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.secondary)
            } else if info.limitDollars == nil {
                Text(info.usageDollars.map { String(format: "$%.4f used", $0) } ?? "Unlimited credits")
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
                            .fill(keyInfoBarColor(fraction: fraction, highColor: AppTheme.Tint.success))
                            .frame(width: geo.size.width * fraction, height: Layout.barHeight)
                    }
                }
                .frame(height: Layout.barHeight)
            }
        }
    }

    private var tierRow: some View {
        HStack(spacing: Layout.hStackSpacing) {
            tierChip
            if let requests = info.requestsPerInterval, let interval = info.rateInterval {
                Text("\(requests) req/\(interval)")
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
        }
    }

    private var tierChip: some View {
        KeyInfoTierChip(
            label: info.isFreeTier ? "Free tier" : "Paid",
            isFreeTier: info.isFreeTier,
            paidColor: AppTheme.Tint.success
        )
    }

    // MARK: - Helpers

    private var accessibilityDescription: String {
        var parts = ["Key validated"]
        if let label = info.label, !label.isEmpty { parts.append(label) }
        if let remaining = info.remainingLabel { parts.append(remaining) }
        parts.append(info.isFreeTier ? "Free tier" : "Paid account")
        if let req = info.requestsPerInterval, let interval = info.rateInterval {
            parts.append("\(req) requests per \(interval)")
        }
        return parts.joined(separator: ", ")
    }
}
