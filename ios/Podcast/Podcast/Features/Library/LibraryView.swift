import SwiftUI

// MARK: - LibraryView

/// Top-level Library tab. Shows a podcast grid from the NMP kernel snapshot;
/// tapping a cell pushes `ShowDetailView` through the NavigationStack.
struct LibraryView: View {
    @Environment(KernelModel.self) private var model
    @Environment(SpotlightDeepLinkRouter.self) private var deepLinkRouter

    @State private var showAddSheet = false
    @State private var path = NavigationPath()
    @State private var showVoiceMode = false

    private let columns = [GridItem(.adaptive(minimum: 140), spacing: AppTheme.Spacing.md)]

    var body: some View {
        NavigationStack(path: $path) {
            Group {
                if model.library.isEmpty {
                    emptyState
                } else {
                    podcastGrid
                }
            }
            .navigationTitle("Library")
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    if !model.library.isEmpty {
                        NavigationLink {
                            AllEpisodesView()
                        } label: {
                            Image(systemName: "list.bullet")
                        }
                        .accessibilityLabel("All Episodes")
                    }
                }
                ToolbarItem(placement: .topBarLeading) {
                    if !model.library.isEmpty {
                        NavigationLink {
                            KnowledgeSearchView()
                        } label: {
                            Image(systemName: "magnifyingglass")
                        }
                        .accessibilityLabel("Search Library")
                        NavigationLink {
                            CategoriesView()
                        } label: {
                            Image(systemName: "square.grid.2x2")
                        }
                        .accessibilityLabel("Browse by Topic")
                        NavigationLink {
                            StarredEpisodesView()
                        } label: {
                            Image(systemName: "bookmark")
                        }
                        .accessibilityLabel("Bookmarks")
                    }
                }
                ToolbarItem(placement: .topBarTrailing) {
                    HStack(spacing: AppTheme.Spacing.sm) {
                        Button { showVoiceMode = true } label: {
                            Image(systemName: "mic.circle.fill")
                        }
                        .accessibilityLabel("Voice mode")
                        .accessibilityIdentifier("library-voice-mode-button")
                        Button { showAddSheet = true } label: {
                            Image(systemName: "plus")
                        }
                    }
                }
            }
            .navigationDestination(for: PodcastSummary.self) { podcast in
                ShowDetailView(podcast: podcast)
            }
            .navigationDestination(for: EpisodeRoute.self) { route in
                EpisodeDetailView(episode: route.episode, podcast: route.podcast)
            }
            .navigationDestination(for: CategoryRoute.self) { route in
                CategoryEpisodesView(category: route.category)
            }
            .refreshable {
                model.dispatch(namespace: "podcast", body: ["op": "refresh_all"])
            }
        }
        .sheet(isPresented: $showAddSheet) {
            AddShowSheet(onDismiss: { showAddSheet = false })
        }
        // Spotlight deep-link consumer. The router holds the tap
        // until we can resolve it — react to both the deep link
        // landing and the library snapshot growing, since cold-start
        // taps may arrive before the kernel has populated the
        // library projection.
        .onChange(of: deepLinkRouter.pendingDeepLink) { _, _ in resolvePendingDeepLink() }
        .onChange(of: model.library) { _, _ in resolvePendingDeepLink() }
        .onAppear { resolvePendingDeepLink() }
        .sheet(isPresented: $showVoiceMode) {
            VoiceModeView()
        }
    }

    /// Attempt to satisfy `deepLinkRouter.pendingDeepLink` against the
    /// current `model.library`. No-op when nothing is pending or when
    /// the referenced row isn't loaded yet (the next snapshot tick or
    /// library mutation will re-run this).
    private func resolvePendingDeepLink() {
        guard let deepLink = deepLinkRouter.pendingDeepLink else { return }
        switch deepLink {
        case .podcast(let id):
            guard let podcast = model.library.first(where: { $0.id == id }) else { return }
            path = NavigationPath()
            path.append(podcast)
            deepLinkRouter.consume()
        case .episode(let id):
            // Episodes live inside `PodcastSummary.episodes`; walk
            // the library once to find the parent show.
            for podcast in model.library {
                if let episode = podcast.episodes.first(where: { $0.id == id }) {
                    path = NavigationPath()
                    path.append(podcast)
                    path.append(EpisodeRoute(episode: episode, podcast: podcast))
                    deepLinkRouter.consume()
                    return
                }
            }
        }
    }

    private var emptyState: some View {
        ContentUnavailableView(
            "No Podcasts Yet",
            systemImage: "headphones",
            description: Text("Tap + to add a show.")
        )
    }

    private var podcastGrid: some View {
        ScrollView {
            LazyVGrid(columns: columns, spacing: AppTheme.Spacing.md) {
                ForEach(model.library) { podcast in
                    NavigationLink(value: podcast) {
                        LibraryGridCell(podcast: podcast)
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(AppTheme.Spacing.md)
        }
    }
}
