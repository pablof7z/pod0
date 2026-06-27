import SwiftUI

// MARK: - LibrarySegment

private enum LibrarySegment: String, CaseIterable, Identifiable {
    case shows
    case episodes

    var id: String { rawValue }

    var label: String {
        switch self {
        case .shows:    return "Shows"
        case .episodes: return "Episodes"
        }
    }
}

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
}

// MARK: - AllEpisodesView

/// Library screen. Hosts a segmented Shows / Episodes picker.
///
/// - **Shows** — podcast artwork grid with unplayed-count badges, a filter
///   rail (All / Unplayed / Downloaded / Transcribed) and a sort menu
///   (Name / Recent / Unplayed count).
/// - **Episodes** — newest-first episode list across all subscriptions with
///   filter chips and scroll-triggered pagination.
struct AllEpisodesView: View {
    @Environment(AppStateStore.self) private var store

    @AppStorage("library.segment") private var segment: LibrarySegment = .shows

    // Episodes-segment state
    @State private var episodeFilter: AllEpisodesFilter = .all
    @State private var searchText: String = ""
    @State private var isSearchActive: Bool = false
    @State private var visibleCount: Int = 50
    @State private var voiceOverDetailRoute: LibraryEpisodeRoute?

    var body: some View {
        Group {
            switch segment {
            case .shows:
                LibraryShowsView()
                    .background(Color(.systemGroupedBackground).ignoresSafeArea())
            case .episodes:
                episodesContent
            }
        }
        .navigationTitle("Library")
        .navigationBarTitleDisplayMode(.large)
        .toolbar { allToolbarItems }
        .navigationDestination(for: Podcast.self) { podcast in
            ShowDetailView(podcast: podcast)
        }
        .navigationDestination(for: LibraryEpisodeRoute.self) { route in
            LibraryEpisodePlaceholder(route: route)
        }
        .navigationDestination(item: $voiceOverDetailRoute) { route in
            LibraryEpisodePlaceholder(route: route)
        }
    }

    // MARK: - Episodes segment

    @ViewBuilder
    private var episodesContent: some View {
        let podcasts = podcastsByID
        let projection = AllEpisodesProjection.load(
            filter: episodeFilter,
            query: searchText,
            limit: visibleCount,
            store: store
        )
        let visible = projection.episodes(in: store)

        List {
            filterRailSection
            episodeListSection(
                visible: visible,
                totalCount: projection.totalCount,
                podcasts: podcasts
            )
        }
        .listStyle(.plain)
        .scrollContentBackground(.hidden)
        .background(Color(.systemGroupedBackground).ignoresSafeArea())
        .searchable(
            text: $searchText,
            isPresented: $isSearchActive,
            placement: .navigationBarDrawer(displayMode: .always),
            prompt: "Search episodes"
        )
        .onChange(of: episodeFilter) { _, _ in visibleCount = 50 }
        .onChange(of: searchText) { _, _ in visibleCount = 50 }
    }

    // MARK: - Toolbar

    @ToolbarContentBuilder
    private var allToolbarItems: some ToolbarContent {
        ToolbarItem(placement: .principal) {
            Picker("", selection: $segment) {
                ForEach(LibrarySegment.allCases) { s in
                    Text(s.label).tag(s)
                }
            }
            .pickerStyle(.segmented)
            .frame(width: 160)
        }
        if segment == .shows {
            ToolbarItem(placement: .topBarTrailing) {
                LibraryShowsSortMenu()
            }
        }
    }

    // MARK: - Computed data

    private var podcastsByID: [UUID: Podcast] {
        Dictionary(uniqueKeysWithValues: store.rustAllPodcasts().map { ($0.id, $0) })
    }

    // MARK: - Episodes filter rail

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
            withAnimation(AppTheme.Animation.springFast) { episodeFilter = f }
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
            .foregroundStyle(episodeFilter == f ? Color.white : Color.primary)
            .background(
                Capsule(style: .continuous)
                    .fill(episodeFilter == f
                          ? AnyShapeStyle(Color.accentColor)
                          : AnyShapeStyle(Color(.tertiarySystemFill)))
            )
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(f.label)
        .accessibilityAddTraits(episodeFilter == f ? .isSelected : [])
    }

    @ViewBuilder
    private func episodeListSection(
        visible: [Episode],
        totalCount: Int,
        podcasts: [UUID: Podcast]
    ) -> some View {
        if visible.isEmpty {
            Section {
                episodesEmptyState
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
    private var episodesEmptyState: some View {
        if !searchText.isEmpty {
            ContentUnavailableView.search(text: searchText)
                .listRowSeparator(.hidden)
                .listRowBackground(Color.clear)
                .padding(.top, AppTheme.Spacing.xl)
        } else {
            ContentUnavailableView(
                episodesEmptyTitle,
                systemImage: episodesEmptyIcon,
                description: Text(episodesEmptySubtitle)
            )
            .listRowSeparator(.hidden)
            .listRowBackground(Color.clear)
            .padding(.top, AppTheme.Spacing.xl)
        }
    }

    private var episodesEmptyTitle: String {
        switch episodeFilter {
        case .all:        return "No episodes yet."
        case .unplayed:   return "Nothing unplayed."
        case .inProgress: return "Nothing in progress."
        case .downloaded: return "No downloads."
        case .starred:    return "No starred episodes."
        }
    }

    private var episodesEmptyIcon: String {
        switch episodeFilter {
        case .all:        return "tray"
        case .unplayed:   return "circle.dashed"
        case .inProgress: return "circle.lefthalf.filled"
        case .downloaded: return "arrow.down.circle"
        case .starred:    return "star"
        }
    }

    private var episodesEmptySubtitle: String {
        switch episodeFilter {
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

// MARK: - LibraryShowsSortMenu

/// Standalone sort-menu button for embedding in the Library Shows toolbar.
/// Kept in this file so it stays within the 500-line hard limit for either
/// file; it is tightly scoped to the Library feature.
private struct LibraryShowsSortMenu: View {
    @AppStorage("library.shows.sort") private var sort: LibraryShowsSort = .recentEpisode

    var body: some View {
        Menu {
            ForEach(LibraryShowsSort.allCases) { s in
                Button {
                    Haptics.selection()
                    withAnimation(AppTheme.Animation.springFast) { sort = s }
                } label: {
                    Label(s.label, systemImage: s.systemImage)
                }
            }
        } label: {
            Label("Sort", systemImage: sortSystemImage)
        }
        .accessibilityLabel("Sort shows")
    }

    private var sortSystemImage: String {
        "arrow.up.arrow.down"
    }
}
