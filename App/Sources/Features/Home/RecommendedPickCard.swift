import SwiftUI

// MARK: - RecommendedPickCard
//
// One card in the #46 "Recommended for you" rail. Renders directly from the
// kernel's `AgentPickSummary` projection — title, podcast name, artwork, and
// the LLM "because …" reason — without depending on the local `Episode`
// having loaded yet (the snapshot carries everything the card needs).
//
// Distinct from `HomeAgentPickCard`, which renders the inbox-triage local
// model. This one is a thin, score-ordered recommendation tile.

struct RecommendedPickCard: View {
    let pick: AgentPickSummary
    let onTap: () -> Void

    private let cardWidth: CGFloat = 220

    var body: some View {
        Button(action: onTap) {
            VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
                artwork
                Text(pick.episodeTitle)
                    .font(AppTheme.Typography.headline)
                    .foregroundStyle(.primary)
                    .lineLimit(2)
                    .multilineTextAlignment(.leading)
                if !pick.podcastTitle.isEmpty {
                    Text(pick.podcastTitle)
                        .font(AppTheme.Typography.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                if !reasonDisplay.isEmpty {
                    Text(reasonDisplay)
                        .font(AppTheme.Typography.subheadline)
                        .italic()
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                        .multilineTextAlignment(.leading)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            .frame(width: cardWidth, alignment: .leading)
            .padding(AppTheme.Spacing.sm)
            .background(
                Color(.secondarySystemBackground),
                in: RoundedRectangle(cornerRadius: AppTheme.Corner.lg, style: .continuous)
            )
            .contentShape(RoundedRectangle(cornerRadius: AppTheme.Corner.lg, style: .continuous))
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .combine)
        .accessibilityLabel(accessibilityLabel)
        .accessibilityHint("Opens episode details")
        .accessibilityAddTraits(.isButton)
    }

    // MARK: - Subviews

    private var artwork: some View {
        let url = pick.artworkUrl.flatMap { URL(string: $0) }
        return ZStack {
            Color.secondary.opacity(0.18)
            if let url {
                CachedAsyncImage(
                    url: url,
                    targetSize: CGSize(width: 256, height: 256)
                ) { phase in
                    switch phase {
                    case .success(let image): image.resizable().scaledToFill()
                    default: placeholderGlyph
                    }
                }
            } else {
                placeholderGlyph
            }
        }
        .frame(width: cardWidth - AppTheme.Spacing.sm * 2, height: cardWidth - AppTheme.Spacing.sm * 2)
        .clipShape(RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous))
    }

    private var placeholderGlyph: some View {
        Image(systemName: "waveform")
            .font(.system(size: 36, weight: .light))
            .foregroundStyle(.secondary)
    }

    // MARK: - Derivation

    /// "Because <reason>" framing, mirroring `HomeAgentPickCard` so the picks
    /// read as editorial recommendations. Elides a duplicate preamble when the
    /// model already opened with "because". Empty reason ⇒ no rationale line.
    private var reasonDisplay: String {
        let trimmed = pick.pickReason.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return "" }
        if trimmed.lowercased().hasPrefix("because") { return trimmed }
        let lowered = trimmed.first.map { String($0).lowercased() + trimmed.dropFirst() } ?? ""
        return "Because \(lowered)"
    }

    private var accessibilityLabel: String {
        var parts: [String] = []
        if !pick.podcastTitle.isEmpty { parts.append(pick.podcastTitle) }
        parts.append(pick.episodeTitle)
        if !pick.pickReason.isEmpty { parts.append(pick.pickReason) }
        parts.append("Recommended pick")
        return parts.joined(separator: ", ")
    }
}

// MARK: - Preview

#Preview {
    let picks = [
        AgentPickSummary(
            episodeId: UUID().uuidString,
            episodeTitle: "How to Think About Keto",
            podcastId: UUID().uuidString,
            podcastTitle: "The Tim Ferriss Show",
            pickReason: "you've been listening to a lot of metabolic-health episodes lately",
            pickScore: 0.92
        ),
        AgentPickSummary(
            episodeId: UUID().uuidString,
            episodeTitle: "The Future of On-Device AI",
            podcastId: UUID().uuidString,
            podcastTitle: "Latent Space",
            pickReason: "it follows up on the inference-cost thread from last week",
            pickScore: 0.81
        )
    ]
    return ScrollView(.horizontal) {
        HStack(spacing: 16) {
            ForEach(picks) { pick in
                RecommendedPickCard(pick: pick, onTap: {})
            }
        }
        .padding()
    }
}
