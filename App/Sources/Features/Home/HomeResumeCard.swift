import SwiftUI

// MARK: - HomeResumeCard

/// Featured-section "resume" card species. Full-bleed cover artwork, a
/// scrubber line drawn over the bottom, and a monospaced time-left caption.
/// Visually distinct from the agent-pick card so the two species don't
/// blend together in the rail.
struct HomeResumeCard: View {
    let episode: Episode
    let podcast: Podcast?
    let onPlay: () -> Void

    @Environment(AppStateStore.self) private var store

    private static let cardWidth: CGFloat = 220
    private static let artHeight: CGFloat = 220

    var body: some View {
        Button(action: onPlay) {
            VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
                artwork
                meta
            }
            .frame(width: Self.cardWidth)
            .padding(AppTheme.Spacing.sm)
            .background(
                Color(.secondarySystemBackground),
                in: RoundedRectangle(cornerRadius: AppTheme.Corner.lg, style: .continuous)
            )
            .contentShape(RoundedRectangle(cornerRadius: AppTheme.Corner.lg, style: .continuous))
        }
        .buttonStyle(.plain)
        .contextMenu {
            EpisodeRowContextMenu(
                episode: episode,
                store: store,
                openDetailsRoute: HomeEpisodeRoute(episodeID: episode.id)
            )
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel(accessibilityLabel)
        .accessibilityHint("Resumes this episode")
        .accessibilityAddTraits(.isButton)
    }

    // MARK: - Subviews

    private var artworkURL: URL? {
        episode.imageURL ?? podcast?.imageURL
    }

    @ViewBuilder
    private var artwork: some View {
        ZStack(alignment: .bottom) {
            ZStack {
                Color.secondary.opacity(0.18)
                if let url = artworkURL {
                    CachedAsyncImage(
                        url: url,
                        targetSize: CGSize(width: 440, height: 440)
                    ) { phase in
                        switch phase {
                        case .success(let image): image.resizable().scaledToFill()
                        default: artworkPlaceholder
                        }
                    }
                } else {
                    artworkPlaceholder
                }
            }
            .frame(width: Self.cardWidth - AppTheme.Spacing.sm * 2, height: Self.artHeight)
            .clipShape(RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous))

            scrubberLine
                .padding(.horizontal, AppTheme.Spacing.sm)
                .padding(.bottom, AppTheme.Spacing.sm)
        }
    }

    private var artworkPlaceholder: some View {
        Image(systemName: "waveform")
            .font(.system(size: 36, weight: .light))
            .foregroundStyle(.secondary)
    }

    private var scrubberLine: some View {
        GeometryReader { geo in
            ZStack(alignment: .leading) {
                Capsule()
                    .fill(Color.black.opacity(0.35))
                    .frame(height: 3)
                Capsule()
                    .fill(Color.white)
                    .frame(width: geo.size.width * progressFraction, height: 3)
            }
        }
        .frame(height: 3)
    }

    private var meta: some View {
        VStack(alignment: .leading, spacing: 2) {
            if let showName = podcast?.title, !showName.isEmpty {
                Text(showName)
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Text(episode.title)
                .font(AppTheme.Typography.headline)
                .foregroundStyle(.primary)
                .lineLimit(2)
                .multilineTextAlignment(.leading)
            Text(remainingLabel)
                .font(AppTheme.Typography.mono)
                .foregroundStyle(.secondary)
        }
    }

    // MARK: - Helpers

    private var progressFraction: Double {
        guard let duration = episode.duration, duration > 0 else { return 0 }
        return max(0.02, min(1, episode.playbackPosition / duration))
    }

    private var remainingLabel: String {
        guard let duration = episode.duration, duration > 0 else { return "Resume" }
        let remaining = max(0, duration - episode.playbackPosition)
        let total = Int(remaining.rounded())
        let h = total / 3600
        let m = (total % 3600) / 60
        if h > 0 { return "\(h)h \(m)m left" }
        if m > 0 { return "\(m) min left" }
        return "<1 min left"
    }

    private var accessibilityLabel: String {
        var parts: [String] = []
        if let s = podcast?.title, !s.isEmpty { parts.append(s) }
        parts.append(episode.title)
        let percent = Int((progressFraction * 100).rounded())
        if percent > 0 { parts.append("\(percent) percent listened") }
        parts.append(remainingLabel)
        return parts.joined(separator: ", ")
    }
}
