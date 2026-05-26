import SwiftUI

// MARK: - ShowDetailHeader

/// Hero header for `ShowDetailView`. Square artwork on the leading edge with
/// title, author, and episode count stacked to the right.
struct ShowDetailHeader: View {
    let podcast: PodcastSummary

    private static let artworkSize: CGFloat = 116

    var body: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.md) {
            HStack(alignment: .top, spacing: AppTheme.Spacing.md) {
                artwork

                VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
                    Text(podcast.title)
                        .font(AppTheme.Typography.title)
                        .lineLimit(2)
                        .fixedSize(horizontal: false, vertical: true)

                    if let author = podcast.author, !author.isEmpty {
                        Text(author)
                            .font(AppTheme.Typography.subheadline)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }

                    metaRow
                        .padding(.top, AppTheme.Spacing.xs)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }

            if let desc = podcast.description, !desc.isEmpty {
                Text(desc)
                    .font(AppTheme.Typography.body)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.leading)
                    .lineLimit(4)
                    .fixedSize(horizontal: false, vertical: true)
                    .textSelection(.enabled)
            }
        }
        .padding(.horizontal, AppTheme.Spacing.lg)
        .padding(.top, AppTheme.Spacing.lg)
        .padding(.bottom, AppTheme.Spacing.md)
    }

    // MARK: - Pieces

    private var artwork: some View {
        RoundedRectangle(cornerRadius: AppTheme.Corner.lg, style: .continuous)
            .fill(Color.accentColor.opacity(0.35))
            .overlay(artworkOverlay)
            .frame(width: Self.artworkSize, height: Self.artworkSize)
            .clipShape(RoundedRectangle(cornerRadius: AppTheme.Corner.lg, style: .continuous))
            .appShadow(AppTheme.Shadow.lifted)
    }

    @ViewBuilder
    private var artworkOverlay: some View {
        if let urlStr = podcast.artworkUrl, let url = URL(string: urlStr) {
            AsyncImage(url: url) { phase in
                switch phase {
                case .success(let image):
                    image.resizable().scaledToFill()
                default:
                    artworkSymbol
                }
            }
        } else {
            artworkSymbol
        }
    }

    private var artworkSymbol: some View {
        Image(systemName: "headphones")
            .font(.system(size: 44, weight: .light))
            .foregroundStyle(.white.opacity(0.92))
            .accessibilityHidden(true)
    }

    private var metaRow: some View {
        let count = podcast.episodeCount
        return Text("\(count) \(count == 1 ? "episode" : "episodes")")
            .font(AppTheme.Typography.caption)
            .foregroundStyle(.secondary)
    }
}
