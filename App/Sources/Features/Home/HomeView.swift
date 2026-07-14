import SwiftUI

// MARK: - HomeView
//
// Merged Home — replaces the old Today + Library tabs with a single
// editorial surface:
//   • Dateline + active-filter chip strip
//   • Featured (resume cards + agent picks), collapsible
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
    @AppStorage("home.featuredExpanded") private var featuredExpanded: Bool = true

    @State private var triageService = InboxTriageService.shared
    @State private var threadingService = ThreadingInferenceService.shared
    @State private var unsubscribeTarget: Podcast?
    @State private var relatedSheetEpisode: Episode?
    @State private var threadedTodaySheet: ThreadingInferenceService.ActiveTopic?
    @State private var voiceOverDetailRoute: HomeEpisodeRoute?
    @State private var showAddShowSheet: Bool = false
    @State private var showCategoryPicker: Bool = false
    @State private var showAllContinueListening: Bool = false
    @State private var showAllPodcasts: Bool = false
    @State private var showInbox: Bool = false
    /// Cached "now" used by the dateline + recency pills. Pinned at body
    /// composition time so a 1Hz playback tick doesn't re-format the
    /// recency pill on every redraw.
    @State private var renderedAt: Date = Date()
    @State private var cachedTopActiveThread: ThreadingInferenceService.ActiveTopic?

    var body: some View {
        scrollContent
            .navigationTitle(navBarTitle)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { toolbarContent }
            .background(Color(.systemGroupedBackground).ignoresSafeArea())
            .refreshable { await refreshAllFeeds() }
            .navigationDestination(for: HomeEpisodeRoute.self) { route in
                EpisodeDetailView(episodeID: route.episodeID)
            }
            .navigationDestination(item: $voiceOverDetailRoute) { route in
                EpisodeDetailView(episodeID: route.episodeID)
            }
            .navigationDestination(for: Podcast.self) { pod in
                ShowDetailView(podcast: pod)
            }
            .navigationDestination(isPresented: $showAllContinueListening) {
                ContinueListeningView(episodes: continueListeningEpisodes)
            }
            .navigationDestination(isPresented: $showAllPodcasts) {
                AllPodcastsListView()
            }
            .navigationDestination(isPresented: $showInbox) {
                InboxView(allowedSubscriptionIDs: allowedSubscriptionIDs)
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
            .sheet(item: $relatedSheetEpisode) { episode in
                HomeRelatedSheet(
                    seedEpisode: episode,
                    seedPodcast: store.podcast(id: episode.podcastID)
                )
                .presentationDetents([.medium, .large])
                .presentationDragIndicator(.visible)
            }
            .sheet(item: $threadedTodaySheet) { active in
                HomeThreadedTodayView(active: active)
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
            .task {
                // Kick AI Inbox triage so freshly-arrived episodes get a
                // decision. Coalesced — concurrent calls all wait on a
                // single in-flight pass. Category changes don't need to
                // re-trigger this since triage decisions are persisted
                // on episodes and the Inbox bundle just filters them.
                triageService.triageNewEpisodes(store: store)
                // Bind the threading service to the store so the
                // "Threaded Today" derivation has somewhere to look.
                threadingService.attach(store: store)
            }
            .onAppear { renderedAt = Date() }
            .task(id: topActiveThreadKey) {
                cachedTopActiveThread = threadingService.topActiveTopics(
                    limit: 1,
                    subscriptionFilter: allowedSubscriptionIDs
                ).first
            }
    }

    private var topActiveThread: ThreadingInferenceService.ActiveTopic? { cachedTopActiveThread }

    private struct TopActiveThreadKey: Equatable {
        var episodeCount: Int
        var totalUnplayed: Int
        var mentionCount: Int
        var categoryID: UUID?
    }

    private var topActiveThreadKey: TopActiveThreadKey {
        TopActiveThreadKey(
            episodeCount: store.state.episodes.count,
            totalUnplayed: store.unplayedCountByShow.values.reduce(0, +),
            mentionCount: store.state.threadingMentions.count,
            categoryID: selectedCategoryID
        )
    }

    /// Subscription-id set for the active category, or `nil` for All.
    /// Resolved once and passed down so the featured surface, dateline,
    /// and threaded-today rail all narrow to the same set of shows.
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

    /// Roll-up of the agent's triage decisions for the subtitle under the
    /// Inbox section header. Scopes counts to the active category so the
    /// line reads consistently with the magazine-mode UI. Unplayed-only on
    /// the inbox side — listened episodes drop off the surface anyway, so
    /// counting them reads as stale.
    private var triageCounts: (inbox: Int, archived: Int, shows: Int) {
        let allowed = allowedSubscriptionIDs
        var inbox = 0
        var archived = 0
        var coveredShows: Set<UUID> = []
        for episode in store.state.episodes {
            if let allowed, !allowed.contains(episode.podcastID) { continue }
            guard let decision = episode.triageDecision else { continue }
            coveredShows.insert(episode.podcastID)
            switch decision {
            case .inbox:
                if !episode.played { inbox += 1 }
            case .archived:
                archived += 1
            }
        }
        return (inbox, archived, coveredShows.count)
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

                if shouldShowInboxSection {
                    let triage = triageCounts
                    HomeFeaturedSection(
                        picksBundle: inboxBundle,
                        isStreaming: triageService.isRunning && inboxBundle.picks.isEmpty,
                        activeThread: topActiveThread,
                        activeCategoryID: selectedCategoryID,
                        activeCategoryName: activeCategory?.name,
                        inboxCount: triage.inbox,
                        archivedCount: triage.archived,
                        showCount: triage.shows,
                        lastTriagedAt: triageService.lastCompletedAt,
                        isExpanded: $featuredExpanded,
                        onPlayEpisode: playEpisode,
                        onLongPressEpisode: { relatedSheetEpisode = $0 },
                        onOpenThread: { threadedTodaySheet = topActiveThread },
                        onSeeAll: { showInbox = true }
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
        if store.state.subscriptions.isEmpty {
            VStack(spacing: AppTheme.Spacing.lg) {
                HomeFirstRunEmptyState(onAddShow: { showAddShowSheet = true })
                // Even with zero follows the user can have podcasts in the
                // library — agent external plays, OPML rows whose subs were
                // later removed, etc. Surface an "All Podcasts" entry so the
                // new screen is reachable in that case too.
                if hasUnfollowedPodcasts {
                    Button {
                        Haptics.selection()
                        showAllPodcasts = true
                    } label: {
                        Label("See all podcasts", systemImage: "list.bullet.rectangle")
                            .font(AppTheme.Typography.subheadline.weight(.semibold))
                    }
                    .buttonStyle(.bordered)
                }
            }
            .padding(.top, AppTheme.Spacing.xl)
        } else if filteredSubs.isEmpty {
            HomeFilteredEmptyState(
                filter: filter,
                categoryName: activeCategory?.name,
                onClearFilters: {
                    categoryFilterID = ""
                    filter = .all
                }
            )
            .padding(.top, AppTheme.Spacing.xl)
        } else {
            HomeSubscriptionListSection(
                podcasts: filteredSubs,
                now: renderedAt,
                onRequestUnsubscribe: { unsubscribeTarget = $0 },
                onSeeAllPodcasts: { showAllPodcasts = true }
            )
        }
    }

    /// `true` when the library contains at least one real podcast row the
    /// user does NOT actively follow. Drives the All-Podcasts affordance in
    /// the no-subscriptions empty state — without this, the screen would
    /// be reachable only after the user follows something, which defeats
    /// the point of surfacing unfollowed shows.
    private var hasUnfollowedPodcasts: Bool {
        let followed = Set(store.state.subscriptions.map(\.podcastID))
        return store.allPodcasts.contains {
            $0.id != Podcast.unknownID && !followed.contains($0.id)
        }
    }

    // MARK: - Filter derivation
    //
    // Filters apply to the subscription list ONLY — featured is curated.
    // Pure derivation kept inline so the `body` getter stays straightforward
    // without an extra service indirection for trivial in-memory work.

    private var filteredSubs: [Podcast] {
        let recencySorted = store.sortedFollowedPodcastsByRecency
        let categoryScoped = applyCategoryFilter(recencySorted)
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

    /// Persisted Inbox bundle for the currently-active category. The
    /// triage service writes `.inbox` decisions onto episodes; this
    /// composes the bundle by filtering + sorting them and is therefore
    /// cheap to recompute on every body pass.
    private var inboxBundle: HomeAgentPicksBundle {
        HomeInboxBundleBuilder.make(
            store: store,
            allowedSubscriptionIDs: allowedSubscriptionIDs,
            now: renderedAt
        )
    }

    private var shouldShowInboxSection: Bool {
        !inboxBundle.picks.isEmpty || triageService.isRunning
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
    }

    // MARK: - Actions

    private func playEpisode(_ episode: Episode) {
        Haptics.medium()
        playback.setEpisode(episode)
        playback.play()
    }

    private func refreshAllFeeds() async {
        await SubscriptionRefreshService.shared.refreshAll(store: store)
        // `refreshAll` already kicks `InboxTriageService.triageNewEpisodes`
        // after the upsert sweep, so any new episodes get classified on
        // this pass. The triage service coalesces, so a second call here
        // is harmless but unnecessary.
    }
}
