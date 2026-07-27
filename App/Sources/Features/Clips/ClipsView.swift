import SwiftUI

// MARK: - ClipsView

/// Two segments over the same material: the clips you kept, and everything
/// you have written against them. Tapping a clip opens it inside the
/// conversation it came from rather than seeking the player there — playing is
/// a separate, deliberate act.
struct ClipsView: View {

    private enum Segment: Hashable {
        case clips
        case notes
    }

    @State private var segment: Segment = .clips
    @State private var searchQuery = ""
    @State private var showsSearch = false
    @State private var isSearchPresented = false
    @State private var episodeNavTarget: UUID?
    @State private var clipNavTarget: UUID?

    var body: some View {
        ZStack {
            if showsSearch {
                content
                    .searchable(
                        text: $searchQuery,
                        isPresented: $isSearchPresented,
                        placement: .navigationBarDrawer(displayMode: .automatic),
                        prompt: searchPrompt
                    )
            } else {
                content
            }
        }
        .background(Color(.systemBackground).ignoresSafeArea())
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(Color(.systemBackground), for: .navigationBar)
        .toolbarBackground(.visible, for: .navigationBar)
        .toolbar { ToolbarItem(placement: .principal) { picker } }
        .navigationDestination(item: $episodeNavTarget) { id in
            EpisodeDetailView(episodeID: id)
        }
        .navigationDestination(item: $clipNavTarget) { id in
            ClipTranscriptView(clipID: id)
        }
        .onChange(of: isSearchPresented) { _, presented in
            if !presented, searchQuery.isEmpty {
                showsSearch = false
            }
        }
    }

    // MARK: - Segments

    /// Lives in the navigation toolbar rather than inline above the list. The
    /// inline segmented control was deliberately removed from this screen when
    /// Clips stopped carrying the old Saved/Starred split, and putting a new
    /// one back in the content would undo that cleanup.
    private var picker: some View {
        LiquidGlassSegmentedPicker(
            "Clips segment",
            selection: $segment,
            segments: [(.clips, "Clips"), (.notes, "Notes")]
        )
        .frame(width: 190)
    }

    @ViewBuilder
    private var content: some View {
        switch segment {
        case .clips:
            ClipsSegment(
                searchQuery: searchQuery,
                onOpenEpisode: { episodeNavTarget = $0 },
                onOpenClip: { clipNavTarget = $0 },
                onPullToSearch: revealSearch
            )
        case .notes:
            NotesSegment(
                searchQuery: searchQuery,
                onOpenEpisode: { episodeNavTarget = $0 },
                onOpenClip: { clipNavTarget = $0 }
            )
        }
    }

    private var searchPrompt: String {
        segment == .clips ? "Search clips" : "Search notes"
    }

    private func revealSearch() {
        guard !showsSearch else { return }
        showsSearch = true
        Task { @MainActor in
            isSearchPresented = true
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
    Task {
        await store.addClip(Clip(
            episodeID: episode.id,
            subscriptionID: podcast.id,
            startMs: 14 * 60_000 + 31_000,
            endMs: 14 * 60_000 + 58_000,
            caption: "On metabolism",
            transcriptText: "Metabolic flexibility isn't a diet — it's a property of the mitochondria.",
            source: .touch
        ))
    }
    return NavigationStack {
        ClipsView()
            .environment(store)
            .environment(PlaybackState())
    }
}
#endif
