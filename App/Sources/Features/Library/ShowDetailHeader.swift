import SwiftUI

// MARK: - ShowDetailHeader

/// Hero header for `ShowDetailView` — square artwork on the leading edge with
/// title, author, description (3-line cap), and meta row stacked to the right.
///
/// **Tint:** the screen-level gradient lives in `ShowDetailView` so it can
/// bleed past the safe area / nav bar; the header itself is matte and sits
/// on top of that gradient.
///
/// **Glass:** none. The header is a matte editorial surface.
struct ShowDetailHeader: View {
    let podcast: Podcast
    let episodeCount: Int

    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    private static let artworkSize: CGFloat = 116

    var body: some View {
        Group {
            if dynamicTypeSize.isAccessibilitySize {
                accessibilityLayout
            } else {
                compactLayout
            }
        }
        .padding(.horizontal, AppTheme.Spacing.lg)
        .padding(.top, AppTheme.Spacing.lg)
        .padding(.bottom, AppTheme.Spacing.md)
    }

    private var compactLayout: some View {
        HStack(alignment: .top, spacing: AppTheme.Spacing.md) {
            artwork

            VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
                titleBlock
                descriptionBlock
                metaRow
                    .padding(.top, AppTheme.Spacing.xs)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private var accessibilityLayout: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.md) {
            HStack(alignment: .top, spacing: AppTheme.Spacing.md) {
                artwork
                titleBlock
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            descriptionBlock
            metaRow
        }
    }

    // MARK: - Pieces

    private var titleBlock: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
            Text(podcast.title)
                .font(AppTheme.Typography.title)
                .lineLimit(2)
                .fixedSize(horizontal: false, vertical: true)

            if !podcast.author.isEmpty {
                Text(podcast.author)
                    .font(AppTheme.Typography.subheadline)
                    .foregroundStyle(.secondary)
                    .lineLimit(dynamicTypeSize.isAccessibilitySize ? 2 : 1)
            }
        }
    }

    @ViewBuilder
    private var descriptionBlock: some View {
        let body = podcast.plainTextDescription
        if !body.isEmpty {
            Text(body)
                .font(AppTheme.Typography.caption)
                .foregroundStyle(.secondary)
                .lineLimit(3)
                .fixedSize(horizontal: false, vertical: true)
                .padding(.top, AppTheme.Spacing.xs)
        }
    }

    private var artwork: some View {
        RoundedRectangle(cornerRadius: AppTheme.Corner.lg, style: .continuous)
            .fill(
                LinearGradient(
                    colors: [
                        podcast.accentColor.opacity(0.95),
                        podcast.accentColor.opacity(0.55)
                    ],
                    startPoint: .topLeading,
                    endPoint: .bottomTrailing
                )
            )
            .overlay(artworkOverlay)
            .frame(width: Self.artworkSize, height: Self.artworkSize)
            .clipShape(RoundedRectangle(cornerRadius: AppTheme.Corner.lg, style: .continuous))
            .appShadow(AppTheme.Shadow.lifted)
    }

    @ViewBuilder
    private var artworkOverlay: some View {
        if let url = podcast.imageURL {
            CachedAsyncImage(url: url) { phase in
                switch phase {
                case .success(let image):
                    image
                        .resizable()
                        .scaledToFill()
                default:
                    artworkSymbol
                }
            }
        } else {
            artworkSymbol
        }
    }

    private var artworkSymbol: some View {
        Image(systemName: podcast.artworkSymbol)
            .font(.system(size: 44, weight: .light))
            .foregroundStyle(.white.opacity(0.92))
            .accessibilityHidden(true)
    }

    @ViewBuilder
    private var metaRow: some View {
        if dynamicTypeSize.isAccessibilitySize {
            VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
                episodeCountLabel
                refreshedLabel
            }
        } else {
            HStack(spacing: AppTheme.Spacing.sm) {
                episodeCountLabel
                if podcast.lastRefreshedAt != nil {
                    Text("·")
                        .font(AppTheme.Typography.caption)
                        .foregroundStyle(.tertiary)
                }
                refreshedLabel
            }
        }
    }

    private var episodeCountLabel: some View {
        Text("\(episodeCount) \(episodeCount == 1 ? "episode" : "episodes")")
            .font(AppTheme.Typography.caption)
            .foregroundStyle(.secondary)
            .lineLimit(1)
    }

    @ViewBuilder
    private var refreshedLabel: some View {
        if let refreshed = podcast.lastRefreshedAt {
            Text("Updated \(relative(refreshed))")
                .font(AppTheme.Typography.caption)
                .foregroundStyle(.secondary)
                .lineLimit(1)
        }
    }

    private func relative(_ date: Date) -> String {
        Self.relativeFormatter.localizedString(for: date, relativeTo: Date())
    }

    private static let relativeFormatter: RelativeDateTimeFormatter = {
        let f = RelativeDateTimeFormatter()
        f.unitsStyle = .abbreviated
        return f
    }()
}
