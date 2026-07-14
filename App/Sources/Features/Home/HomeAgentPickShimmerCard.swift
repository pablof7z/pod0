import SwiftUI

// MARK: - HomeAgentPickShimmerCard
//
// Placeholder card rendered in the featured rail while the LLM is still
// streaming the next pick. Mirrors the shape of `HomeAgentPickCard` so the
// rail's geometry stays stable when the real card crossfades in.
//
// Uses SwiftUI's built-in `.redacted(reason: .placeholder)` modifier to
// produce the standard system shimmer instead of hand-rolling a gradient —
// the FeedbackView skeleton state uses the same pattern.

struct HomeAgentPickShimmerCard: View {
    let isHero: Bool

    @State private var animatePulse: Bool = false

    var body: some View {
        HStack(alignment: .top, spacing: 0) {
            Rectangle()
                .fill(AppTheme.Tint.agentSurface.opacity(isHero ? 0.95 : 0.65))
                .frame(width: 2)
            content
                .padding(AppTheme.Spacing.sm)
        }
        .frame(width: isHero ? 320 : 240, alignment: .leading)
        .background(
            Color(.secondarySystemBackground),
            in: RoundedRectangle(cornerRadius: AppTheme.Corner.lg, style: .continuous)
        )
        .redacted(reason: .placeholder)
        .opacity(animatePulse ? 0.65 : 1.0)
        .animation(
            .easeInOut(duration: 0.9).repeatForever(autoreverses: true),
            value: animatePulse
        )
        .onAppear { animatePulse = true }
        .accessibilityLabel("Loading next pick")
    }

    @ViewBuilder
    private var content: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
            HStack(alignment: .top, spacing: AppTheme.Spacing.sm) {
                RoundedRectangle(cornerRadius: AppTheme.Corner.sm, style: .continuous)
                    .fill(Color(.tertiarySystemFill))
                    .frame(width: isHero ? 64 : 44, height: isHero ? 64 : 44)
                VStack(alignment: .leading, spacing: 4) {
                    Text("Podcast title placeholder")
                        .font(AppTheme.Typography.caption)
                        .lineLimit(1)
                    Text("Episode title placeholder that wraps across two lines")
                        .font(isHero ? AppTheme.Typography.title3 : AppTheme.Typography.headline)
                        .lineLimit(2)
                }
            }
            Text("Because rationale placeholder line.")
                .font(AppTheme.Typography.subheadline)
                .lineLimit(1)
        }
    }
}
