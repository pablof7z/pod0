import SwiftUI
import os
import os.signpost

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

    @State private var threadingService = ThreadingInferenceService.shared
    @State private var unsubscribeTarget: Podcast?
    @State private var relatedSheetEpisode: Episode?
    @State private var threadedTodaySheet: ThreadingInferenceService.ActiveTopic?
    @State private var voiceOverDetailRoute: HomeEpisodeRoute?
    /// Navigation route pushed when the user taps a kernel-scored pick in the
    /// #46 "Recommended for you" rail.
    @State private var pickRoute: HomeEpisodeRoute?
    @State private var showAddShowSheet: Bool = false
    @State private var showCategoryPicker: Bool = false
    @State private var showAllContinueListening: Bool = false
    @Binding var showAllPodcasts: Bool
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
            .navigationDestination(item: $pickRoute) { route in
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
                // Ask the kernel to (re)triage so freshly-arrived episodes
                // get a decision. The Rust kernel owns triage (M5): it
                // selects candidates, runs the classifier, and projects the
                // decisions onto `Episode.triageDecision`. Swift only
                // signals "recompute" and displays the result — category
                // changes don't need to re-trigger since the Inbox bundle
                // just filters the projected decisions.
                store.kernelTriageInbox()
                // Bind the threading service to the store so the
                // "Threaded Today" derivation has somewhere to look.
                threadingService.attach(store: store)
            }
            .onAppear { renderedAt = Date() }
            .task(id: topActiveThreadKey) {
                store.refreshThreadingProjection()
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
            episodeCount: store.rustEpisodeCount(),
            totalUnplayed: store.rustTotalUnplayedCount(),
            mentionCount: store.threadingProjection.mentions.count,
            categoryID: selectedCategoryID
        )
    }

    private var categoryProjection: CategoryLibraryProjection {
        CategoryLibraryProjection.load(categories: store.state.categories, store: store)
    }

    /// Subscription-id set for the active category, or `nil` for All.
    /// Rust resolves valid category membership; Swift passes the renderer
    /// scope through to Rust-owned Home projections and native row builders.
    private var allowedSubscriptionIDs: Set<UUID>? {
        guard let id = selectedCategoryID else { return nil }
        return Set(categoryProjection.podcastIDsByCategory[id] ?? [])
    }

    /// Resolved `PodcastCategory` for the active filter, or `nil` for All.
    private var activeCategory: PodcastCategory? {
        guard let id = selectedCategoryID else { return nil }
        return store.category(id: id)
    }

    /// Roll-up of the agent's triage decisions for the subtitle under the
    /// Inbox section header. Rust owns the count semantics and active-category
    /// scope; Swift passes only the renderer's podcast-id scope and displays
    /// the returned values.
    private var triageCounts: (inbox: Int, archived: Int, shows: Int) {
        let interval = signposter.beginInterval("triageCounts")
        defer { signposter.endInterval("triageCounts", interval) }
        let podcastIDs = allowedSubscriptionIDs.map { Array($0) } ?? []
        let decoder = JSONDecoder()
        guard let envelope = store.kernel?.homeTriageRollupEnvelope(podcastIDs: podcastIDs),
              let data = envelope.data(using: .utf8),
              let decoded = try? decoder.decode(HomeTriageRollupEnvelope.self, from: data)
        else { return (0, 0, 0) }
        return (decoded.inbox, decoded.archived, decoded.shows)
    }

    private var inboxLastTriagedAt: Date? {
        guard let timestamp = store.kernel?.podcastSnapshot?.inboxLastTriagedAt else {
            return nil
        }
        return Date(timeIntervalSince1970: TimeInterval(timestamp))
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

                if !recommendedPicks.isEmpty {
                    HomeRecommendedSection(
                        picks: recommendedPicks,
                        onSelect: { pick in
                            guard let id = UUID(uuidString: pick.episodeId) else { return }
                            Haptics.selection()
                            pickRoute = HomeEpisodeRoute(episodeID: id)
                        }
                    )
                }

                if shouldShowInboxSection {
                    let triage = triageCounts
                    HomeFeaturedSection(
                        picksBundle: inboxBundle,
                        isStreaming: store.kernel?.podcastSnapshot?.inboxTriageInProgress ?? false,
                        activeThread: topActiveThread,
                        activeCategoryID: selectedCategoryID,
                        activeCategoryName: activeCategory?.name,
                        inboxCount: triage.inbox,
                        archivedCount: triage.archived,
                        showCount: triage.shows,
                        lastTriagedAt: inboxLastTriagedAt,
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

    /// In-progress episodes for the Continue Listening section. Rust owns the
    /// product filter (unplayed, non-archived, started, last two weeks, active
    /// category scope) and returns ordered episode ids; Swift resolves them for
    /// native row rendering.
    private var continueListeningEpisodes: [Episode] {
        let podcastIDs = allowedSubscriptionIDs.map { Array($0) } ?? []
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        guard let envelope = store.kernel?.homeContinueListeningEnvelope(limit: 20, podcastIDs: podcastIDs),
              let data = envelope.data(using: .utf8),
              let decoded = try? decoder.decode(HomeContinueListeningEnvelope.self, from: data)
        else { return [] }
        return decoded.episodeIds
            .compactMap { UUID(uuidString: $0) }
            .compactMap { store.episode(id: $0) }
    }

    // MARK: - Subscription surface

    @ViewBuilder
    private var subscriptionsSurface: some View {
        if store.rustFollowedPodcastCount() == 0 {
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
        store.rustHasUnfollowedPodcasts()
    }

    // MARK: - Filter derivation
    //
    // Filters apply to the subscription list ONLY — featured is curated.
    // Rust owns subscription visibility and ordering; Swift passes the active
    // filter/category scope and resolves the returned ids for native rows.

    private var filteredSubs: [Podcast] {
        let podcastIDs = allowedSubscriptionIDs.map { Array($0) } ?? []
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        guard let envelope = store.kernel?.homeSubscriptionListEnvelope(
            filter: filter.rawValue,
            podcastIDs: podcastIDs
        ),
              let data = envelope.data(using: .utf8),
              let decoded = try? decoder.decode(HomeSubscriptionListEnvelope.self, from: data)
        else { return [] }
        return decoded.podcastIds
            .compactMap { UUID(uuidString: $0) }
            .compactMap { store.podcast(id: $0) }
    }

    private var selectedCategoryID: UUID? {
        guard let id = UUID(uuidString: categoryFilterID),
              categoryProjection.categoryIDs.contains(id) else { return nil }
        return id
    }

    private var navBarTitle: String {
        activeCategory?.name ?? "Home"
    }

    /// Persisted Inbox bundle for the currently-active category. The Rust
    /// kernel writes `.inbox` decisions onto episodes via the snapshot
    /// projection; this composes the bundle by filtering + sorting them and
    /// is therefore cheap to recompute on every body pass.
    private var inboxBundle: HomeAgentPicksBundle {
        HomeInboxBundleBuilder.make(
            store: store,
            allowedSubscriptionIDs: allowedSubscriptionIDs,
            now: renderedAt
        )
    }

    private var shouldShowInboxSection: Bool {
        !inboxBundle.picks.isEmpty
    }

    /// #46 — kernel-scored episode recommendations (`PodcastUpdate.picks`),
    /// read straight off the live snapshot. Picks are ephemeral kernel output
    /// folded into `podcastSnapshot`'s content hash, so reading them here (the
    /// same way `EpisodeDetailView` reads `downloads`) makes the rail recompute
    /// whenever the kernel re-scores. Rust owns pick ordering/score
    /// normalization; Swift only applies the active category renderer scope.
    /// Empty ⇒ the section is hidden by the `scrollContent` guard.
    private var recommendedPicks: [AgentPickSummary] {
        let picks = store.kernel?.podcastSnapshot?.picks ?? []
        if let allowed = allowedSubscriptionIDs {
            return picks.filter { pick in
                guard let podcastUUID = UUID(uuidString: pick.podcastId) else { return false }
                return allowed.contains(podcastUUID)
            }
        }
        return picks
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
        store.kernelRefreshAll()
    }
}

private struct HomeContinueListeningEnvelope: Decodable {
    var episodeIds: [String] = []

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        episodeIds = try c.decodeIfPresent([String].self, forKey: .episodeIds) ?? []
    }
}

private struct HomeTriageRollupEnvelope: Decodable {
    var inbox: Int = 0
    var archived: Int = 0
    var shows: Int = 0
}

private struct HomeSubscriptionListEnvelope: Decodable {
    var podcastIds: [String] = []
}
