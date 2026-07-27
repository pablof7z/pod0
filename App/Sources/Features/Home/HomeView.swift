import SwiftUI

// MARK: - HomeView
//
// Merged Home — replaces the old Today + Library tabs with a single
// editorial surface:
//   • Dateline + active-filter chip strip
//   • Continue Listening for recent in-progress episodes
//   • Subscription list, recency-sorted, filterable
//
// Persistence keys mirror what `LibraryView` used so the user's chosen
// filter / category carries over without a one-time reset.
//
// Search lives on its own tab. The earlier inline search-entry bar was
// removed — it duplicated the tab-bar affordance and burned vertical
// space in the editorial scroll.

struct HomeView: View {
    @Environment(AppStateStore.self) private var store
    @Environment(PlaybackState.self) private var playback

    @AppStorage("library.filter") private var filter: LibraryFilter = .all
    @AppStorage("library.categoryFilterID") private var categoryFilterID: String = ""

    @State private var unsubscribeTarget: Podcast?
    /// Set when the user asks to remove an *unfollowed* podcast. Kept
    /// separate from `unsubscribeTarget` so each confirmation can use the
    /// wording that matches what actually happens.
    @State private var deleteTarget: Podcast?
    @State private var showAddShowSheet: Bool = false
    @State private var showCategoryPicker: Bool = false
    @State private var showAllContinueListening: Bool = false
    /// Cached "now" used by the dateline + recency pills. Pinned at body
    /// composition time so a 1Hz playback tick doesn't re-format the
    /// recency pill on every redraw.
    @State private var renderedAt: Date = Date()
    var body: some View {
        scrollContent
            .navigationTitle(navBarTitle)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { toolbarContent }
            .background(Color(.systemGroupedBackground).ignoresSafeArea())
            .refreshable { await refreshAllFeeds() }
            .navigationDestination(for: Podcast.self) { pod in
                ShowDetailView(podcast: pod)
            }
            .navigationDestination(isPresented: $showAllContinueListening) {
                ContinueListeningView(episodes: continueListeningEpisodes)
            }
            .sheet(isPresented: $showAddShowSheet) {
                AddShowSheet(store: store, onDismiss: { showAddShowSheet = false })
            }
            .sheet(isPresented: $showCategoryPicker) {
                HomeCategoryPickerSheet(
                    selectedCategoryID: selectedCategoryID,
                    onSelect: { id in
                        categoryFilterID = id?.uuidString ?? ""
                    }
                )
                .presentationDetents([.medium, .large])
                .presentationDragIndicator(.visible)
            }
            .alert(
                "Unsubscribe from \(unsubscribeTarget?.title ?? "")?",
                isPresented: Binding(
                    get: { unsubscribeTarget != nil },
                    set: { if !$0 { unsubscribeTarget = nil } }
                ),
                presenting: unsubscribeTarget
            ) { sub in
                Button("Cancel", role: .cancel) {}
                Button("Unsubscribe", role: .destructive) {
                    Haptics.warning()
                    store.deletePodcast(podcastID: sub.id)
                }
            } message: { _ in
                Text("This removes the show and all its episodes from your library.")
            }
            .alert(
                "Delete \(deleteTarget?.title ?? "")?",
                isPresented: Binding(
                    get: { deleteTarget != nil },
                    set: { if !$0 { deleteTarget = nil } }
                ),
                presenting: deleteTarget
            ) { pod in
                Button("Cancel", role: .cancel) {}
                Button("Delete", role: .destructive) {
                    Haptics.warning()
                    store.deletePodcast(podcastID: pod.id)
                }
            } message: { _ in
                Text("This removes the podcast and all its episodes from your library.")
            }
            .onAppear { renderedAt = Date() }
    }

    /// Subscription-id set for the active category, or `nil` for All.
    /// Used to keep Continue Listening aligned with the selected category.
    private var allowedSubscriptionIDs: Set<UUID>? {
        guard let id = selectedCategoryID,
              let category = store.category(id: id) else { return nil }
        return Set(category.subscriptionIDs)
    }

    /// Resolved `PodcastCategory` for the active filter, or `nil` for All.
    private var activeCategory: PodcastCategory? {
        guard let id = selectedCategoryID else { return nil }
        return store.category(id: id)
    }

    // MARK: - Layout

    @ViewBuilder
    private var scrollContent: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: AppTheme.Spacing.md) {
                if !continueListeningEpisodes.isEmpty {
                    HomeContinueListeningSection(
                        episodes: continueListeningEpisodes,
                        onPlay: playEpisode,
                        onRemove: { store.resetEpisodeProgress($0.id) },
                        onSeeAll: { showAllContinueListening = true }
                    )
                }

                subscriptionsSurface
                    .padding(.bottom, AppTheme.Spacing.xl)
            }
        }
    }

    /// In-progress episodes from the last 2 weeks, scoped to the active
    /// category. Used by the Continue Listening section.
    private var continueListeningEpisodes: [Episode] {
        let twoWeeksAgo = Date().addingTimeInterval(-14 * 24 * 3600)
        let scoped = HomeCategoryScope.episodesInCategory(
            store.inProgressEpisodes,
            allowedSubscriptionIDs: allowedSubscriptionIDs
        )
        return scoped.filter { $0.pubDate >= twoWeeksAgo }
    }

    // MARK: - Subscription surface

    @ViewBuilder
    private var subscriptionsSurface: some View {
        if !filteredSubs.isEmpty || !filteredUnfollowed.isEmpty {
            HomeSubscriptionListSection(
                podcasts: filteredSubs,
                unfollowedPodcasts: filteredUnfollowed,
                now: renderedAt,
                onRequestUnsubscribe: { unsubscribeTarget = $0 },
                onRequestDelete: { deleteTarget = $0 }
            )
        } else if store.state.subscriptions.isEmpty && unfollowedPodcasts.isEmpty {
            HomeFirstRunEmptyState(onAddShow: { showAddShowSheet = true })
                .padding(.top, AppTheme.Spacing.xl)
        } else {
            HomeFilteredEmptyState(
                filter: filter,
                categoryName: activeCategory?.name,
                onClearFilters: {
                    categoryFilterID = ""
                    filter = .all
                }
            )
            .padding(.top, AppTheme.Spacing.xl)
        }
    }

    // MARK: - Filter derivation
    //
    // Filters apply to the subscription list only.
    // Pure derivation kept inline so the `body` getter stays straightforward
    // without an extra service indirection for trivial in-memory work.

    private var filteredSubs: [Podcast] {
        applyFilters(store.sortedFollowedPodcastsByRecency)
    }

    /// Podcasts in the library the user does not follow, recency-sorted.
    /// Rendered below the followed shows so they stay reachable — nothing
    /// else in the app lists them.
    private var unfollowedPodcasts: [Podcast] {
        store.sortedUnfollowedPodcastsByRecency
    }

    /// Unfollowed rows under the active filters. A category holds
    /// *subscription* ids, so selecting one necessarily empties this list —
    /// an unfollowed podcast cannot be filed in a category.
    private var filteredUnfollowed: [Podcast] {
        applyFilters(unfollowedPodcasts)
    }

    private func applyFilters(_ podcasts: [Podcast]) -> [Podcast] {
        let categoryScoped = applyCategoryFilter(podcasts)
        switch filter {
        case .all:         return categoryScoped
        case .unplayed:    return categoryScoped.filter { store.unplayedCount(forPodcast: $0.id) > 0 }
        case .downloaded:  return categoryScoped.filter { store.hasDownloadedEpisode(forPodcast: $0.id) }
        case .transcribed: return categoryScoped.filter { store.hasTranscribedEpisode(forPodcast: $0.id) }
        }
    }

    private func applyCategoryFilter(_ subs: [Podcast]) -> [Podcast] {
        guard let id = selectedCategoryID,
              let category = store.category(id: id) else { return subs }
        let allowed = Set(category.subscriptionIDs)
        return subs.filter { allowed.contains($0.id) }
    }

    private var selectedCategoryID: UUID? {
        guard let id = UUID(uuidString: categoryFilterID),
              store.category(id: id) != nil else { return nil }
        return id
    }

    private var navBarTitle: String {
        activeCategory?.name ?? "Home"
    }

    // MARK: - Toolbar

    @ToolbarContentBuilder
    private var toolbarContent: some ToolbarContent {
        ToolbarItem(placement: .principal) {
            Button {
                Haptics.light()
                showCategoryPicker = true
            } label: {
                HStack(spacing: 3) {
                    Text(navBarTitle)
                        .font(.system(.headline, design: .rounded, weight: .semibold))
                        .foregroundStyle(.primary)
                    Image(systemName: "chevron.down")
                        .font(.system(size: 10, weight: .bold))
                        .foregroundStyle(.secondary)
                }
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Browse categories")
            .accessibilityHint("Opens category picker")
        }
        // The first-run empty state also offers "Add show", but it only
        // renders when the library is completely empty — a library holding
        // nothing but unfollowed podcasts would otherwise have no way to
        // add one.
        ToolbarItem(placement: .topBarTrailing) {
            Button {
                Haptics.light()
                showAddShowSheet = true
            } label: {
                Image(systemName: "plus")
            }
            .accessibilityLabel("Add show")
        }
    }

    // MARK: - Actions

    private func playEpisode(_ episode: Episode) {
        Haptics.medium()
        playback.setEpisode(episode)
        playback.play()
    }

    private func refreshAllFeeds() async {
        await SubscriptionRefreshService.shared.refreshAll(store: store)
    }
}
