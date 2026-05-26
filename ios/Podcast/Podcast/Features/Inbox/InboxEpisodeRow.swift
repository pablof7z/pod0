import SwiftUI

// Single inbox row. Pure renderer: receives an `InboxItem` and draws it.
// All policy (which episodes qualify, score, caption) lives kernel-side
// in `crate::inbox_handler`.
struct InboxEpisodeRow: View {
    let item: InboxItem

    private static let thumbnailSize: CGFloat = 56

    var body: some View {
        HStack(alignment: .center, spacing: AppTheme.Spacing.md) {
            thumbnail

            VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
                HStack(alignment: .firstTextBaseline, spacing: AppTheme.Spacing.xs) {
                    priorityDot
                    Text(item.episodeTitle)
                        .font(AppTheme.Typography.headline)
                        .lineLimit(2)
                }

                Text(item.podcastTitle)
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)

                if let reason = item.priorityReason {
                    Text(reason)
                        .font(AppTheme.Typography.caption)
                        .foregroundStyle(priorityColor)
                        .lineLimit(1)
                }

                metaRow
            }
        }
        .padding(.vertical, AppTheme.Spacing.sm)
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
        .accessibilityLabel(accessibilityLabel)
    }

    // MARK: - Artwork

    private var artworkURL: URL? {
        guard let s = item.artworkUrl else { return nil }
        return URL(string: s)
    }

    @ViewBuilder
    private var thumbnail: some View {
        let shape = RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous)
        Group {
            if let url = artworkURL {
                AsyncImage(url: url) { phase in
                    switch phase {
                    case .success(let image): image.resizable().scaledToFill()
                    default: placeholder
                    }
                }
            } else {
                placeholder
            }
        }
        .frame(width: Self.thumbnailSize, height: Self.thumbnailSize)
        .clipShape(shape)
        .accessibilityHidden(true)
    }

    private var placeholder: some View {
        ZStack {
            Color.secondary.opacity(0.18)
            Image(systemName: "waveform")
                .font(.system(size: 20, weight: .light))
                .foregroundStyle(.secondary)
        }
    }

    // MARK: - Priority

    /// Score buckets matching `crate::inbox_handler::score` thresholds:
    /// >= 0.75 → high (red), >= 0.5 → medium (yellow), else low (green).
    private var priorityColor: Color {
        switch item.priorityScore {
        case let s where s >= 0.75: return .red
        case let s where s >= 0.5: return .yellow
        default: return .green
        }
    }

    private var priorityDot: some View {
        Circle()
            .fill(priorityColor)
            .frame(width: 8, height: 8)
            .accessibilityHidden(true)
    }

    // MARK: - Meta

    @ViewBuilder
    private var metaRow: some View {
        HStack(spacing: AppTheme.Spacing.sm) {
            if let secs = item.durationSecs {
                Text(formatDuration(secs))
                    .font(AppTheme.Typography.monoCaption)
                    .foregroundStyle(.secondary)
                Text("·")
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.tertiary)
            }
            Text(relativeDate(from: item.publishedAt))
                .font(AppTheme.Typography.caption)
                .foregroundStyle(.secondary)
        }
    }

    private var accessibilityLabel: String {
        var parts = [item.episodeTitle, item.podcastTitle]
        if let reason = item.priorityReason { parts.append(reason) }
        if let secs = item.durationSecs { parts.append(formatDuration(secs)) }
        parts.append(relativeDate(from: item.publishedAt))
        return parts.joined(separator: ", ")
    }
}
