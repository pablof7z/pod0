import SwiftUI

// MARK: - CategoriesView

/// "Browse by Topic" grid surfaced from the Library tab.
///
/// Reads `model.podcastSnapshot?.categories ?? []` and renders one
/// `CategoryCard` per topic the Rust categorizer assigned. Tapping a
/// card pushes `CategoryEpisodesView` into the navigation stack the
/// `LibraryView` already owns.
///
/// Per D7 this view never decides which categories exist or how many
/// labels each episode picks up — the kernel computes both and the
/// shell only renders. The "no categories yet" empty state is the
/// pre-first-refresh / pre-first-subscription case; once the auto-
/// categorize hook fires after a feed refresh, rows appear without
/// any further user action.
struct CategoriesView: View {
    @Environment(KernelModel.self) private var model

    private let columns = [GridItem(.adaptive(minimum: 160), spacing: AppTheme.Spacing.md)]

    var body: some View {
        Group {
            if categories.isEmpty {
                emptyState
            } else {
                grid
            }
        }
        .navigationTitle("Browse by Topic")
        .navigationBarTitleDisplayMode(.inline)
    }

    private var categories: [CategoryBrowseItem] {
        model.podcastSnapshot?.categories ?? []
    }

    private var emptyState: some View {
        ContentUnavailableView(
            "No Topics Yet",
            systemImage: "square.grid.2x2",
            description: Text("Subscribe to a few shows and pull to refresh; the agent will categorize new episodes automatically.")
        )
    }

    private var grid: some View {
        ScrollView {
            LazyVGrid(columns: columns, spacing: AppTheme.Spacing.md) {
                ForEach(categories) { item in
                    NavigationLink(value: CategoryRoute(category: item.category)) {
                        CategoryCard(item: item)
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(AppTheme.Spacing.md)
        }
    }
}

/// Navigation value pushed when the user taps a category card. Kept
/// separate from `PodcastSummary` / `EpisodeRoute` so the enclosing
/// `LibraryView` `navigationDestination` can dispatch on type.
struct CategoryRoute: Hashable {
    let category: String
}

// MARK: - CategoryCard

/// One cell in the [`CategoriesView`] grid. Shows category name,
/// episode count, and a stack of up to three artwork thumbnails for
/// the most-recent episodes.
private struct CategoryCard: View {
    @Environment(KernelModel.self) private var model
    let item: CategoryBrowseItem

    var body: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
            artworkStack
            VStack(alignment: .leading, spacing: 2) {
                Text(item.category)
                    .font(AppTheme.Typography.headline)
                    .foregroundStyle(.primary)
                    .lineLimit(1)
                Text(countLine)
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel("\(item.category), \(countLine)")
    }

    private var countLine: String {
        let episodes = item.episodeCount == 1 ? "1 episode" : "\(item.episodeCount) episodes"
        let podcasts = item.podcastCount == 1 ? "1 show" : "\(item.podcastCount) shows"
        return "\(episodes) · \(podcasts)"
    }

    private var artworkStack: some View {
        ZStack {
            RoundedRectangle(cornerRadius: AppTheme.Corner.lg, style: .continuous)
                .fill(Color.accentColor.opacity(0.18))
                .aspectRatio(1, contentMode: .fit)
            thumbnailRow
                .padding(AppTheme.Spacing.sm)
        }
        .clipShape(RoundedRectangle(cornerRadius: AppTheme.Corner.lg, style: .continuous))
        .appShadow(AppTheme.Shadow.subtle)
    }

    private var thumbnailRow: some View {
        HStack(spacing: -10) {
            ForEach(Array(previewArtworkURLs.prefix(3).enumerated()), id: \.offset) { _, url in
                CategoryThumbnail(url: url)
            }
            if previewArtworkURLs.isEmpty {
                Image(systemName: categoryGlyph)
                    .font(.system(size: 36, weight: .light))
                    .foregroundStyle(Color.accentColor)
                    .accessibilityHidden(true)
            }
            Spacer(minLength: 0)
        }
    }

    private var previewArtworkURLs: [URL] {
        guard let library = model.podcastSnapshot?.library else { return [] }
        let lookup: [String: EpisodeSummary] = library.reduce(into: [:]) { acc, podcast in
            for ep in podcast.episodes { acc[ep.id] = ep }
        }
        let podcastLookup: [String: PodcastSummary] = library.reduce(into: [:]) { acc, podcast in
            acc[podcast.id] = podcast
        }
        return item.topEpisodeIds.compactMap { id in
            guard let ep = lookup[id] else { return nil }
            if let s = ep.artworkUrl, let u = URL(string: s) { return u }
            if let pid = ep.podcastId, let p = podcastLookup[pid],
               let s = p.artworkUrl, let u = URL(string: s) { return u }
            return nil
        }
    }

    private var categoryGlyph: String {
        switch item.category {
        case "Technology": return "cpu"
        case "Science": return "atom"
        case "Business": return "briefcase"
        case "Politics": return "building.columns"
        case "Health": return "heart"
        case "Culture": return "paintpalette"
        case "Sports": return "sportscourt"
        case "Education": return "book"
        case "Entertainment": return "tv"
        default: return "tag"
        }
    }
}

private struct CategoryThumbnail: View {
    let url: URL
    var body: some View {
        AsyncImage(url: url) { phase in
            switch phase {
            case .success(let image): image.resizable().scaledToFill()
            default: Color.secondary.opacity(0.18)
            }
        }
        .frame(width: 48, height: 48)
        .clipShape(RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous)
                .stroke(Color.white.opacity(0.6), lineWidth: 1.5)
        )
    }
}
