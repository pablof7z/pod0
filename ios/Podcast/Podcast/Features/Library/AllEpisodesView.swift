import SwiftUI

// MARK: - AllEpisodesFilter

enum AllEpisodesFilter: String, CaseIterable, Identifiable {
    case all
    case unplayed
    case inProgress
    case downloaded
    case starred

    var id: String { rawValue }

    var label: String {
        switch self {
        case .all:        return "All"
        case .unplayed:   return "Unplayed"
        case .inProgress: return "In Progress"
        case .downloaded: return "Downloaded"
        case .starred:    return "Starred"
        }
    }

    var systemImage: String? {
        switch self {
        case .all:        return nil
        case .unplayed:   return "circle.fill"
        case .inProgress: return "circle.lefthalf.filled"
        case .downloaded: return "arrow.down.circle.fill"
        case .starred:    return "star.fill"
        }
    }

    func matches(_ episode: Episode) -> Bool {
        switch self {
        case .all:        return true
        case .unplayed:   return !episode.played && !episode.isInProgress
        case .inProgress: return episode.isInProgress
        case .downloaded:
            if case .downloaded = episode.downloadState { return true }
            return false
        case .starred:    return episode.isStarred
        }
    }
}

// MARK: - AllEpisodesView

/// Library screen showing every episode across all subscriptions, newest
/// first, with filter chips and scroll-triggered pagination so large libraries
/// never render more rows than are needed.
struct AllEpisodesView: View {
    @Environment(AppStateStore.self) private var store

    @State private var filter: AllEpisodesFilter = .all
    @State private var searchText: String = ""
    @State private var isSearchActive: Bool = false
    @State private var visibleCount: Int = 50
    @State private var voiceOverDetailRoute: LibraryEpisodeRoute?

    var body: some View {
        let podcasts = podcastsByID
        let filtered = filteredEpisodes
        let visible = Array(filtered.prefix(visibleCount))

        List {
            filterRailSection
            episodeListSection(
                visible: visible,
                totalCount: filtered.count,
                podcasts: podcasts
            )
        }
        .listStyle(.plain)
        .scrollContentBackground(.hidden)
        .background(Color(.systemGroupedBackground).ignoresSafeArea())
        .navigationTitle("Library")
        .navigationBarTitleDisplayMode(.large)
        .searchable(
            text: $searchText,
            isPresented: $isSearchActive,
            placement: .navigationBarDrawer(displayMode: .always),
            prompt: "Search episodes"
        )
        .navigationDestination(for: LibraryEpisodeRoute.self) { route in
            LibraryEpisodePlaceholder(route: route)
        }
        .navigationDestination(item: $voiceOverDetailRoute) { route in
            LibraryEpisodePlaceholder(route: route)
        }
        .onChange(of: filter) { _, _ in visibleCount = 50 }
        .onChange(of: searchText) { _, _ in visibleCount = 50 }
    }

    // MARK: - Computed data

    private var podcastsByID: [UUID: Podcast] {
        Dictionary(uniqueKeysWithValues: store.allPodcasts.map { ($0.id, $0) })
    }

    private var filteredEpisodes: [Episode] {
        let byFilter = store.allEpisodesSorted.filter { filter.matches($0) }
        guard !searchText.isEmpty else { return byFilter }
        return byFilter.filter {
            $0.title.localizedCaseInsensitiveContains(searchText) ||
            $0.description.localizedCaseInsensitiveContains(searchText)
        }
    }

    // MARK: - Sections

    private var filterRailSection: some View {
        Section {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: AppTheme.Spacing.sm) {
                    ForEach(AllEpisodesFilter.allCases) { f in
                        filterChip(f)
                    }
                }
                .padding(.horizontal, AppTheme.Spacing.md)
                .padding(.vertical, AppTheme.Spacing.sm)
            }
        }
        .listRowInsets(EdgeInsets())
        .listRowSeparator(.hidden)
        .listRowBackground(Color(.systemGroupedBackground))
    }

    private func filterChip(_ f: AllEpisodesFilter) -> some View {
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

    @ViewBuilder
    private func episodeListSection(
        visible: [Episode],
        totalCount: Int,
        podcasts: [UUID: Podcast]
    ) -> some View {
        if visible.isEmpty {
            Section {
                emptyState
            }
        } else {
            Section {
                AllEpisodesEpisodeList(
                    episodes: visible,
                    podcastsByID: podcasts,
                    voiceOverDetailRoute: $voiceOverDetailRoute,
                    visibleCount: $visibleCount,
                    totalCount: totalCount
                )
            } footer: {
                if visibleCount < totalCount {
                    ProgressView()
                        .frame(maxWidth: .infinity)
                        .padding()
                }
            }
        }
    }

    @ViewBuilder
    private var emptyState: some View {
        if !searchText.isEmpty {
            ContentUnavailableView.search(text: searchText)
                .listRowSeparator(.hidden)
                .listRowBackground(Color.clear)
                .padding(.top, AppTheme.Spacing.xl)
        } else {
            ContentUnavailableView(
                emptyStateTitle,
                systemImage: emptyStateIcon,
                description: Text(emptyStateSubtitle)
            )
            .listRowSeparator(.hidden)
            .listRowBackground(Color.clear)
            .padding(.top, AppTheme.Spacing.xl)
        }
    }

    private var emptyStateTitle: String {
        switch filter {
        case .all:        return "No episodes yet."
        case .unplayed:   return "Nothing unplayed."
        case .inProgress: return "Nothing in progress."
        case .downloaded: return "No downloads."
        case .starred:    return "No starred episodes."
        }
    }

    private var emptyStateIcon: String {
        switch filter {
        case .all:        return "tray"
        case .unplayed:   return "circle.dashed"
        case .inProgress: return "circle.lefthalf.filled"
        case .downloaded: return "arrow.down.circle"
        case .starred:    return "star"
        }
    }

    private var emptyStateSubtitle: String {
        switch filter {
        case .all:
            return "Subscribe to podcasts from the Home tab to see episodes here."
        case .unplayed:
            return "Tap 'All' to see your full library."
        case .inProgress:
            return "Start listening to an episode to see it here."
        case .downloaded:
            return "Download episodes for offline listening."
        case .starred:
            return "Star episodes from the episode detail view."
        }
    }
}
