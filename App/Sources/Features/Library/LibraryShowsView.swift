import SwiftUI

// MARK: - LibraryShowsSort

/// Sort order applied to the subscriptions grid in the Library Shows view.
enum LibraryShowsSort: String, CaseIterable, Identifiable {
    case name
    case recentEpisode
    case unplayedCount

    var id: String { rawValue }

    var label: String {
        switch self {
        case .name:          return "Name"
        case .recentEpisode: return "Recent"
        case .unplayedCount: return "Unplayed"
        }
    }

    var systemImage: String {
        switch self {
        case .name:          return "textformat.abc"
        case .recentEpisode: return "clock.fill"
        case .unplayedCount: return "circle.fill"
        }
    }
}

// MARK: - LibraryShowsFilter

/// Filter applied to the subscriptions grid in the Library Shows view.
///
/// Shows scope ("In Progress" = at least one in-progress episode; "Unplayed" =
/// at least one unplayed episode; "Downloaded" = at least one downloaded
/// episode). Filtering is computed client-side from `LibraryPodcastStatsProjection`.
enum LibraryShowsFilter: String, CaseIterable, Identifiable {
    case all
    case unplayed
    case downloaded
    case inProgress

    var id: String { rawValue }

    var label: String {
        switch self {
        case .all:        return "All"
        case .unplayed:   return "Unplayed"
        case .downloaded: return "Downloaded"
        case .inProgress: return "In Progress"
        }
    }

    var systemImage: String? {
        switch self {
        case .all:        return nil
        case .unplayed:   return "circle.fill"
        case .downloaded: return "arrow.down.circle.fill"
        case .inProgress: return "circle.lefthalf.filled"
        }
    }

    var emptyStateGlyph: String {
        switch self {
        case .all:        return "books.vertical"
        case .unplayed:   return "circle.dashed"
        case .downloaded: return "arrow.down.circle"
        case .inProgress: return "circle.lefthalf.filled"
        }
    }

    var emptyStateTitle: String {
        switch self {
        case .all:        return "Your shows live here."
        case .unplayed:   return "Nothing unplayed."
        case .downloaded: return "No downloaded shows."
        case .inProgress: return "Nothing in progress."
        }
    }

    var emptyStateSubtitle: String {
        switch self {
        case .all:
            return "Search Apple Podcasts, paste a feed URL, or import an OPML file to begin."
        case .unplayed:
            return "Every subscribed show has been listened through. Tap All to see your library."
        case .downloaded:
            return "No episodes are downloaded for offline listening yet."
        case .inProgress:
            return "Start listening to an episode to see it here."
        }
    }
}

// MARK: - LibraryShowsView

/// Subscriptions grid in the Library tab. Shows every followed podcast as
/// artwork cards with an unplayed-count badge, a filter rail
/// (All / Unplayed / Downloaded / In Progress) and a sort menu
/// (Name / Recent / Unplayed count) provided by the parent toolbar.
struct LibraryShowsView: View {
    @Environment(AppStateStore.self) private var store

    @AppStorage("library.shows.filter") private var filter: LibraryShowsFilter = .all
    @AppStorage("library.shows.sort")   private var sort: LibraryShowsSort     = .recentEpisode

    private let columns = [
        GridItem(.adaptive(minimum: 140, maximum: 180), spacing: AppTheme.Spacing.md)
    ]

    var body: some View {
        let podcasts     = filteredAndSorted
        let podcastIDs   = podcasts.map(\.id)
        let libraryStats = LibraryPodcastStatsProjection.load(
            podcastIDs: podcastIDs,
            store: store
        )

        ScrollView {
            filterRail
                .padding(.top, AppTheme.Spacing.xs)

            if podcasts.isEmpty {
                ContentUnavailableView(
                    filter.emptyStateTitle,
                    systemImage: filter.emptyStateGlyph,
                    description: Text(filter.emptyStateSubtitle)
                )
                .padding(.top, AppTheme.Spacing.xl)
            } else {
                LazyVGrid(columns: columns, spacing: AppTheme.Spacing.lg) {
                    ForEach(podcasts) { podcast in
                        NavigationLink(value: podcast) {
                            LibraryGridCell(
                                podcast: podcast,
                                unplayedCount: libraryStats.unplayedCount(for: podcast.id)
                            )
                        }
                        .buttonStyle(.plain)
                        .accessibilityIdentifier("library-shows-cell")
                    }
                }
                .padding(.horizontal, AppTheme.Spacing.md)
                .padding(.bottom, AppTheme.Spacing.xl)
            }
        }
    }

    // MARK: - Filter rail

    private var filterRail: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: AppTheme.Spacing.sm) {
                ForEach(LibraryShowsFilter.allCases) { f in
                    filterChip(f)
                }
            }
            .padding(.horizontal, AppTheme.Spacing.md)
            .padding(.vertical, AppTheme.Spacing.sm)
        }
    }

    private func filterChip(_ f: LibraryShowsFilter) -> some View {
        Button {
            Haptics.selection()
            withAnimation(AppTheme.Animation.springFast) { filter = f }
        } label: {
            HStack(spacing: AppTheme.Spacing.xs) {
                if let symbol = f.systemImage {
                    Image(systemName: symbol)
                        .font(.caption2.weight(.semibold))
                }
                Text(f.label)
                    .font(AppTheme.Typography.caption)
            }
            .padding(.horizontal, AppTheme.Spacing.md)
            .padding(.vertical, AppTheme.Spacing.sm)
            .foregroundStyle(filter == f ? Color.white : Color.primary)
            .background(
                Capsule(style: .continuous)
                    .fill(filter == f
                          ? AnyShapeStyle(Color.accentColor)
                          : AnyShapeStyle(Color(.tertiarySystemFill)))
            )
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(f.label)
        .accessibilityAddTraits(filter == f ? .isSelected : [])
    }

    // MARK: - Data derivation

    private var allFollowedPodcasts: [Podcast] {
        store.rustFollowedPodcasts()
    }

    private var filteredAndSorted: [Podcast] {
        let base = applyFilter(allFollowedPodcasts)
        return applySort(base)
    }

    private func applyFilter(_ podcasts: [Podcast]) -> [Podcast] {
        switch filter {
        case .all:
            return podcasts
        case .unplayed:
            return podcasts.filter { store.unplayedCount(forPodcast: $0.id) > 0 }
        case .downloaded:
            return podcasts.filter { store.hasDownloadedEpisode(forPodcast: $0.id) }
        case .inProgress:
            return podcasts.filter { hasInProgressEpisode(podcastID: $0.id) }
        }
    }

    private func applySort(_ podcasts: [Podcast]) -> [Podcast] {
        switch sort {
        case .name:
            return podcasts.sorted {
                $0.title.localizedCaseInsensitiveCompare($1.title) == .orderedAscending
            }
        case .recentEpisode:
            // `rustFollowedPodcasts()` already returns followed podcasts in
            // recency order; preserve that ordering for this sort key.
            return podcasts
        case .unplayedCount:
            return podcasts.sorted {
                store.unplayedCount(forPodcast: $0.id) >
                store.unplayedCount(forPodcast: $1.id)
            }
        }
    }

    /// `true` when at least one episode for the podcast is in progress
    /// (started but not finished). Uses the episode projection to avoid
    /// loading all episodes into memory — scoped to the first in-progress
    /// episode per show.
    private func hasInProgressEpisode(podcastID: UUID) -> Bool {
        let episodes = store.rustEpisodes(forPodcast: podcastID, limit: 100)
        return episodes.contains { $0.isInProgress }
    }
}
