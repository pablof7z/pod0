import SwiftUI

struct PodcastSearchView: View {
    @Environment(AppStateStore.self) private var store
    @Environment(PlaybackState.self) private var playback
    @State private var model = PodcastSearchViewModel()
    @State private var destination: PodcastSearchDestination?

    private var localResults: PodcastLocalSearchResults {
        PodcastSearchEngine.localResults(query: model.debouncedQuery, store: store)
    }

    /// Kernel knowledge-search results for the current query. Reactively
    /// read from `store.kernel?.podcastSnapshot?.knowledgeSearchResults` —
    /// SwiftUI's `@Observable` tracking re-renders on every new batch.
    private var kernelTranscriptResults: [KnowledgeSearchResult] {
        store.kernel?.podcastSnapshot?.knowledgeSearchResults ?? []
    }

    private var hasAnyResults: Bool {
        !localResults.isEmpty || !kernelTranscriptResults.isEmpty
    }

    private var shouldShowTranscriptSection: Bool {
        model.isSearchingTranscripts || !kernelTranscriptResults.isEmpty
    }

    var body: some View {
        List {
            if model.query.isBlank {
                PodcastSearchPromptEmptyState { example in
                    model.query = example
                }
            } else {
                resultSections
                if !hasAnyResults && !model.isSearchingTranscripts {
                    PodcastSearchNoResultsView(query: model.query)
                }
            }
        }
        .listStyle(.insetGrouped)
        .tabBarMinimizeBehavior(.never)
        .navigationTitle("Search")
        .navigationBarTitleDisplayMode(.large)
        .searchable(
            text: $model.query,
            placement: .navigationBarDrawer(displayMode: .always),
            prompt: "Shows, episodes, transcripts"
        )
        .task(id: model.query) {
            // Keep existing debounce: 300 ms after the last keystroke.
            guard !model.query.trimmed.isEmpty else {
                model.debouncedQuery = ""
                model.searchTranscripts(store: store)
                return
            }
            do {
                try await Task.sleep(nanoseconds: 300_000_000)
            } catch {
                return
            }
            model.debouncedQuery = model.query
            model.searchTranscripts(store: store)
        }
        // Clear the spinner when the kernel projection delivers results.
        // `kernelTranscriptResults` is observed via @Observable — this fires
        // on every new batch without any polling.
        .onChange(of: kernelTranscriptResults) { _, _ in
            model.didReceiveKernelResults()
        }
        .navigationDestination(item: $destination) { destination in
            destinationView(destination)
        }
        .toolbar {
            if hasAnyResults {
                ToolbarItem(placement: .topBarTrailing) {
                    Text("\(resultCount)")
                        .font(AppTheme.Typography.caption)
                        .foregroundStyle(.secondary)
                        .monospacedDigit()
                }
            }
        }
    }

    @ViewBuilder
    private var resultSections: some View {
        if !localResults.shows.isEmpty {
            Section("Shows") {
                ForEach(localResults.shows) { hit in
                    Button {
                        Haptics.selection()
                        destination = .show(hit.podcast.id)
                    } label: {
                        PodcastShowSearchRow(hit: hit, query: model.query)
                    }
                    .buttonStyle(.plain)
                }
            }
        }

        if !localResults.episodes.isEmpty {
            Section("Episodes") {
                ForEach(localResults.episodes) { hit in
                    Button {
                        Haptics.selection()
                        destination = .episode(hit.episode.id)
                    } label: {
                        PodcastEpisodeSearchRow(hit: hit, query: model.query)
                    }
                    .buttonStyle(.plain)
                }
            }
        }

        kernelTranscriptSection
    }

    @ViewBuilder
    private var kernelTranscriptSection: some View {
        if shouldShowTranscriptSection {
            Section("Transcripts") {
                if model.isSearchingTranscripts {
                    ProgressView()
                }
                ForEach(kernelTranscriptResults) { hit in
                    Button {
                        openKernelHit(hit)
                    } label: {
                        PodcastKernelSearchRow(hit: hit, query: model.query)
                    }
                    .buttonStyle(.plain)
                }
            }
        }
    }

    private var resultCount: Int {
        localResults.shows.count
            + localResults.episodes.count
            + kernelTranscriptResults.count
    }

    /// Navigate to an episode from a kernel search hit. When `startSecs` is
    /// available, seeks to that position and starts playback (the same
    /// "play this moment" contract as the old chunk-level transcript hit).
    /// Without `startSecs` (the common case for BM25 episode-level results),
    /// just opens the episode detail.
    private func openKernelHit(_ hit: KnowledgeSearchResult) {
        Haptics.selection()
        guard let uuid = UUID(uuidString: hit.episodeId) else { return }
        if let episode = store.episode(id: uuid), let secs = hit.startSecs, secs > 0 {
            playback.setEpisode(episode)
            playback.seek(to: secs)
            if !playback.isPlaying { playback.play() }
        }
        destination = .episode(uuid)
    }

    @ViewBuilder
    private func destinationView(_ destination: PodcastSearchDestination) -> some View {
        switch destination {
        case .show(let id):
            if let podcast = store.podcast(id: id) {
                ShowDetailView(podcast: podcast)
            } else {
                missingView("Show not found")
            }
        case .episode(let id):
            if store.episode(id: id) != nil {
                EpisodeDetailView(episodeID: id)
            } else {
                missingView("Episode not found")
            }
        }
    }

    private func missingView(_ title: String) -> some View {
        ContentUnavailableView(title, systemImage: "questionmark.folder")
    }
}
