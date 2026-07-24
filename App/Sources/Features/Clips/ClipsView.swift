import SwiftUI

// MARK: - ClipsView

struct ClipsView: View {
    @State private var searchQuery = ""
    @State private var showsSearch = false
    @State private var isSearchPresented = false
    @State private var episodeNavTarget: UUID?

    var body: some View {
        ZStack {
            if showsSearch {
                clipsContent
                    .searchable(
                        text: $searchQuery,
                        isPresented: $isSearchPresented,
                        placement: .navigationBarDrawer(displayMode: .automatic),
                        prompt: "Search clips"
                    )
            } else {
                clipsContent
            }
        }
        .background(Color(.systemBackground).ignoresSafeArea())
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(Color(.systemBackground), for: .navigationBar)
        .toolbarBackground(.visible, for: .navigationBar)
        .navigationDestination(item: $episodeNavTarget) { id in
            EpisodeDetailView(episodeID: id)
        }
        .onChange(of: isSearchPresented) { _, presented in
            if !presented, searchQuery.isEmpty {
                showsSearch = false
            }
        }
    }

    private var clipsContent: some View {
        ClipsSegment(
            searchQuery: searchQuery,
            onOpenEpisode: { episodeNavTarget = $0 },
            onPullToSearch: {
                guard !showsSearch else { return }
                showsSearch = true
                Task { @MainActor in
                    isSearchPresented = true
                }
            }
        )
    }
}

// MARK: - Preview

#if DEBUG
#Preview {
    let podcast = Podcast(
        feedURL: URL(string: "https://example.com/feed")!,
        title: "The Peter Attia Drive"
    )
    let episode = Episode(
        podcastID: podcast.id,
        guid: "preview",
        title: "How to Think About Keto",
        pubDate: Date(),
        enclosureURL: URL(string: "https://example.com/x.mp3")!
    )
    var previewState = AppState()
    previewState.podcasts = [podcast]
    previewState.subscriptions = [PodcastSubscription(podcastID: podcast.id)]
    previewState.episodes = [episode]
    let store = AppStateStore.previewStore(importing: previewState, name: "saved")
    store.addClip(Clip(
        episodeID: episode.id,
        subscriptionID: podcast.id,
        startMs: 14 * 60_000 + 31_000,
        endMs: 14 * 60_000 + 58_000,
        caption: "On metabolism",
        transcriptText: "Metabolic flexibility isn't a diet — it's a property of the mitochondria.",
        source: .touch
    ))
    return NavigationStack {
        ClipsView()
            .environment(store)
            .environment(PlaybackState())
    }
}
#endif
