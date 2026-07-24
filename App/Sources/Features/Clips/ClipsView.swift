import SwiftUI

// MARK: - ClipsView

struct ClipsView: View {
    @State private var searchQuery = ""
    @State private var episodeNavTarget: UUID?

    var body: some View {
        ClipsSegment(
            searchQuery: searchQuery,
            onOpenEpisode: { episodeNavTarget = $0 }
        )
            .navigationTitle("Clips")
            .navigationBarTitleDisplayMode(.large)
            .background(Color(.systemGroupedBackground).ignoresSafeArea())
            .searchable(
                text: $searchQuery,
                placement: .navigationBarDrawer(displayMode: .automatic),
                prompt: "Search clips"
            )
            .navigationDestination(item: $episodeNavTarget) { id in
                EpisodeDetailView(episodeID: id)
            }
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
