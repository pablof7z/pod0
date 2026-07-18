import SwiftUI

// MARK: - SavedView
//
// Merges the former separate Bookmarks and Clippings tabs into one "Saved"
// screen. The two segments show genuinely different row types — a `Clip` is
// a timestamped audio excerpt, while a starred episode is an `Episode` flag
// rollup (star + clip count + note count) — so rather than force them into
// one heterogeneous list, a segmented control switches between two purpose-
// built lists that each keep their prior look and behavior.

struct SavedView: View {

    private enum Segment: Hashable {
        case clips
        case starred
    }

    @Environment(AppStateStore.self) private var store
    @Environment(PlaybackState.self) private var playback

    @State private var segment: Segment = .clips
    @State private var searchQuery = ""
    @State private var episodeNavTarget: UUID?

    var body: some View {
        content
            .navigationTitle("Saved")
            .navigationBarTitleDisplayMode(.large)
            .background(Color(.systemGroupedBackground).ignoresSafeArea())
            .searchable(text: $searchQuery, prompt: segment == .clips ? "Search clips" : "Search saved episodes")
            .safeAreaInset(edge: .top) {
                LiquidGlassSegmentedPicker(
                    "Saved segment",
                    selection: $segment,
                    segments: [(.clips, "Clips"), (.starred, "Starred")]
                )
                .padding(.horizontal, AppTheme.Spacing.lg)
                .padding(.vertical, AppTheme.Spacing.sm)
            }
            .navigationDestination(item: $episodeNavTarget) { id in
                EpisodeDetailView(episodeID: id)
            }
    }

    @ViewBuilder
    private var content: some View {
        switch segment {
        case .clips:
            ClipsSegment(searchQuery: searchQuery, onOpenEpisode: { episodeNavTarget = $0 })
        case .starred:
            StarredSegment(searchQuery: searchQuery, onOpenEpisode: { episodeNavTarget = $0 })
        }
    }
}

// MARK: - Preview

#Preview {
    let store = AppStateStore()
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
    store.upsertPodcast(podcast)
    store.addSubscription(podcastID: podcast.id)
    store.upsertEpisodes([episode], forPodcast: podcast.id)
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
        SavedView()
            .environment(store)
            .environment(PlaybackState())
    }
}
