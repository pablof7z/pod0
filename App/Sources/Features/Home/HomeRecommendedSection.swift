import SwiftUI

// MARK: - HomeRecommendedSection
//
// #46 — surfaces the Rust kernel's scored episode recommendations
// (`PodcastUpdate.picks`, projected as `AgentPickSummary`) as a horizontal
// rail on Home.
//
// This is deliberately a *separate* surface from the inbox-triage
// `HomeFeaturedSection`: that rail renders the local `HomeAgentPicksBundle`
// (inbox decisions, no score), whereas this one renders the kernel's
// LLM-scored picks ordered by `pickScore`. Reusing the inbox model would
// drop the score + podcast title the kernel produces, so the two outputs
// stay distinct (AGENTS.md: one canonical representation per concept).
//
// Picks are ephemeral kernel output — they ride `KernelModel.podcastSnapshot`
// (folded into its content hash) rather than persisted `AppState`, so the
// caller reads them straight off the snapshot and this view stays a pure
// function of its `picks` input. Empty input ⇒ the caller hides the section.

struct HomeRecommendedSection: View {
    /// Kernel-scored picks, already sorted by the caller (highest score first).
    let picks: [AgentPickSummary]
    /// Tap handler — resolves the pick's episode id to a navigation route.
    let onSelect: (AgentPickSummary) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
            header
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: AppTheme.Spacing.md) {
                    ForEach(picks) { pick in
                        RecommendedPickCard(pick: pick) { onSelect(pick) }
                    }
                }
                .padding(.horizontal, AppTheme.Spacing.md)
            }
            .scrollClipDisabled()
        }
    }

    private var header: some View {
        HStack(alignment: .firstTextBaseline, spacing: 6) {
            Image(systemName: "sparkles")
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(Color.accentColor)
            Text("Recommended for you")
                .font(.system(.headline, design: .rounded, weight: .semibold))
                .foregroundStyle(.primary)
        }
        .padding(.horizontal, AppTheme.Spacing.md)
        .accessibilityAddTraits(.isHeader)
    }
}
