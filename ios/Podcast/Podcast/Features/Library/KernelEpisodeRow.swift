import SwiftUI

// MARK: - KernelEpisodeRow

/// Single-episode row backed by `EpisodeSummary`. Used by `ShowDetailEpisodeList`.
struct KernelEpisodeRow: View {
    let episode: EpisodeSummary
    var fallbackArtworkUrl: String? = nil
    let onPlay: () -> Void
    /// `nil` when the episode is already downloaded (renders a check); a
    /// non-nil closure when it isn't (renders a download button).
    let onDownload: (() -> Void)?

    private static let thumbnailSize: CGFloat = 56

    var body: some View {
        HStack(alignment: .center, spacing: AppTheme.Spacing.md) {
            thumbnail

            VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
                HStack(spacing: AppTheme.Spacing.xs) {
                    Text(episode.title)
                        .font(AppTheme.Typography.headline)
                        .lineLimit(2)
                        .foregroundStyle(episode.played ? Color.secondary : Color.primary)
                    if episode.starred {
                        Image(systemName: "bookmark.fill")
                            .font(.caption2)
                            .foregroundStyle(.orange)
                    }
                }

                metaRow
            }

            Spacer()

            downloadIndicator

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

    // MARK: - Download indicator

    @ViewBuilder
    private var downloadIndicator: some View {
        if let onDownload {
            Button {
                onDownload()
            } label: {
                Image(systemName: "arrow.down.circle")
                    .font(.title3)
                    .foregroundStyle(.secondary)
                    .frame(width: 36, height: 36)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Download \(episode.title)")
        } else {
            // Already downloaded — show a calm checkmark, no tap target.
            Image(systemName: "checkmark.circle.fill")
                .font(.title3)
                .foregroundStyle(.secondary)
                .frame(width: 36, height: 36)
                .accessibilityLabel("Downloaded")
        }
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
        let resumeSecs = episode.playbackPositionSecs
        if hasDuration || hasDate || resumeSecs != nil {
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
                if let resumeSecs {
                    if hasDuration || hasDate {
                        Text("·")
                            .font(AppTheme.Typography.caption)
                            .foregroundStyle(.tertiary)
                    }
                    Text("Resume \(formatDuration(resumeSecs))")
                        .font(AppTheme.Typography.monoCaption)
                        .foregroundStyle(Color.accentColor)
                }
            }
        }
    }

    private var accessibilityLabel: String {
        var parts = [episode.title]
        if let secs = episode.durationSecs { parts.append(formatDuration(secs)) }
        if let ts = episode.publishedAt { parts.append(relativeDate(from: ts)) }
        return parts.joined(separator: ", ")
    }
}
