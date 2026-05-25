import SwiftUI

// MARK: - ShowDetailEpisodeList

/// Episode list inside `ShowDetailView`. Renders a `ForEach` of NMP-native
/// episode rows driven by `EpisodeSummary` from the kernel snapshot.
///
/// Play is dispatched directly to the `player` namespace — no `PlaybackState`,
/// no `AppStateStore`. Swipe actions, downloads, and episode-detail navigation
/// arrive in later PRs once those capabilities are ported to Rust.
struct ShowDetailEpisodeList: View {
    let episodes: [EpisodeSummary]
    let podcast: PodcastSummary

    @Environment(KernelModel.self) private var model

    var body: some View {
        ForEach(episodes) { ep in
            KernelEpisodeRow(
                episode: ep,
                fallbackArtworkUrl: podcast.artworkUrl,
                onPlay: {
                    Haptics.medium()
                    model.dispatch(
                        namespace: "podcast.player",
                        body: ["op": "play", "episode_id": ep.id]
                    )
                    NotificationCenter.default.post(name: .openPlayerRequested, object: nil)
                }
            )
            .listRowSeparator(.hidden)
            .listRowInsets(EdgeInsets(
                top: AppTheme.Spacing.xs,
                leading: AppTheme.Spacing.lg,
                bottom: AppTheme.Spacing.xs,
                trailing: AppTheme.Spacing.lg
            ))
            .listRowBackground(Color(.systemBackground))
        }
    }
}

// MARK: - KernelEpisodeRow

/// Single-episode row backed by `EpisodeSummary`. Used by `ShowDetailEpisodeList`.
private struct KernelEpisodeRow: View {
    let episode: EpisodeSummary
    var fallbackArtworkUrl: String? = nil
    let onPlay: () -> Void

    private static let thumbnailSize: CGFloat = 56

    var body: some View {
        HStack(alignment: .center, spacing: AppTheme.Spacing.md) {
            thumbnail

            VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
                Text(episode.title)
                    .font(AppTheme.Typography.headline)
                    .lineLimit(2)

                metaRow
            }

            Spacer()

            Button {
                onPlay()
            } label: {
                Image(systemName: "play.circle.fill")
                    .font(.title2)
                    .foregroundStyle(Color.accentColor)
                    .frame(width: 44, height: 44)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Play \(episode.title)")
        }
        .padding(.vertical, AppTheme.Spacing.sm)
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
        .accessibilityLabel(accessibilityLabel)
        .accessibilityAddTraits(.isButton)
        .onTapGesture { onPlay() }
    }

    // MARK: - Artwork

    private var artworkURL: URL? {
        if let s = episode.artworkUrl, let url = URL(string: s) { return url }
        if let s = fallbackArtworkUrl, let url = URL(string: s) { return url }
        return nil
    }

    @ViewBuilder
    private var thumbnail: some View {
        let shape = RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous)
        Group {
            if let url = artworkURL {
                AsyncImage(url: url) { phase in
                    switch phase {
                    case .success(let image): image.resizable().scaledToFill()
                    default: thumbnailPlaceholder
                    }
                }
            } else {
                thumbnailPlaceholder
            }
        }
        .frame(width: Self.thumbnailSize, height: Self.thumbnailSize)
        .clipShape(shape)
        .accessibilityHidden(true)
    }

    private var thumbnailPlaceholder: some View {
        ZStack {
            Color.secondary.opacity(0.18)
            Image(systemName: "waveform")
                .font(.system(size: 20, weight: .light))
                .foregroundStyle(.secondary)
        }
    }

    // MARK: - Meta

    @ViewBuilder
    private var metaRow: some View {
        let hasDuration = episode.durationSecs != nil
        let hasDate = episode.publishedAt != nil
        if hasDuration || hasDate {
            HStack(spacing: AppTheme.Spacing.sm) {
                if let secs = episode.durationSecs {
                    Text(formatDuration(secs))
                        .font(AppTheme.Typography.monoCaption)
                        .foregroundStyle(.secondary)
                }
                if hasDuration && hasDate {
                    Text("·")
                        .font(AppTheme.Typography.caption)
                        .foregroundStyle(.tertiary)
                }
                if let ts = episode.publishedAt {
                    Text(relativeDate(from: ts))
                        .font(AppTheme.Typography.caption)
                        .foregroundStyle(.secondary)
                }
            }
        }
    }

    private func formatDuration(_ secs: Double) -> String {
        let total = Int(secs)
        let h = total / 3600
        let m = (total % 3600) / 60
        let s = total % 60
        if h > 0 { return String(format: "%d:%02d:%02d", h, m, s) }
        return String(format: "%d:%02d", m, s)
    }

    private func relativeDate(from unixSeconds: Int) -> String {
        let date = Date(timeIntervalSince1970: TimeInterval(unixSeconds))
        return Self.relativeFormatter.localizedString(for: date, relativeTo: Date())
    }

    private static let relativeFormatter: RelativeDateTimeFormatter = {
        let f = RelativeDateTimeFormatter()
        f.unitsStyle = .abbreviated
        return f
    }()

    private var accessibilityLabel: String {
        var parts = [episode.title]
        if let secs = episode.durationSecs { parts.append(formatDuration(secs)) }
        if let ts = episode.publishedAt { parts.append(relativeDate(from: ts)) }
        return parts.joined(separator: ", ")
    }
}
