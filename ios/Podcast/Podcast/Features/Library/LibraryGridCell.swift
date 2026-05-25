import SwiftUI

// MARK: - LibraryGridCell

/// One card in the Library subscriptions grid, driven by `PodcastSummary` from
/// the NMP kernel snapshot.
struct LibraryGridCell: View {
    let podcast: PodcastSummary

    var body: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
            artworkTile
            VStack(alignment: .leading, spacing: 2) {
                Text(podcast.title)
                    .font(AppTheme.Typography.headline)
                    .foregroundStyle(.primary)
                    .lineLimit(2)
                    .multilineTextAlignment(.leading)
                if let author = podcast.author, !author.isEmpty {
                    Text(author)
                        .font(AppTheme.Typography.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel(accessibilityLabel)
    }

    private var artworkTile: some View {
        RoundedRectangle(cornerRadius: AppTheme.Corner.lg, style: .continuous)
            .fill(Color.accentColor.opacity(0.25))
            .aspectRatio(1, contentMode: .fit)
            .overlay(artworkOverlay)
            .clipShape(RoundedRectangle(cornerRadius: AppTheme.Corner.lg, style: .continuous))
            .appShadow(AppTheme.Shadow.subtle)
    }

    @ViewBuilder
    private var artworkOverlay: some View {
        if let urlStr = podcast.artworkUrl, let url = URL(string: urlStr) {
            AsyncImage(url: url) { phase in
                switch phase {
                case .success(let image):
                    image.resizable().scaledToFill()
                default:
                    symbolPlaceholder
                }
            }
        } else {
            symbolPlaceholder
        }
    }

    private var symbolPlaceholder: some View {
        Image(systemName: "headphones")
            .font(.system(size: 44, weight: .light))
            .foregroundStyle(.white.opacity(0.7))
            .accessibilityHidden(true)
    }

    private var accessibilityLabel: String {
        var parts = [podcast.title]
        if let author = podcast.author, !author.isEmpty { parts.append(author) }
        if podcast.unplayedCount > 0 { parts.append("\(podcast.unplayedCount) unplayed") }
        return parts.joined(separator: ", ")
    }
}
