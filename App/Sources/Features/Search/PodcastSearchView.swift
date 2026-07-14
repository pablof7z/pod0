import SwiftUI

struct PodcastSearchView: View {
    @Environment(AppStateStore.self) private var store
    @Environment(PlaybackState.self) private var playback
    @State private var model = PodcastSearchViewModel()
    @State private var destination: PodcastSearchDestination?

    private var localResults: PodcastLocalSearchResults {
        PodcastSearchEngine.localResults(query: model.debouncedQuery, state: store.state)
    }

    private var hasAnyResults: Bool {
        !localResults.isEmpty || !model.transcriptResults.isEmpty || !model.wikiResults.isEmpty
    }

    private var shouldShowTranscriptSection: Bool {
        model.isSearchingTranscripts
            || !model.transcriptResults.isEmpty
            || (model.transcriptError != nil && localResults.isEmpty && model.wikiResults.isEmpty)
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
        .task { await model.loadWikiPages() }
        .task(id: model.query) {
            guard !model.query.trimmed.isEmpty else {
                model.debouncedQuery = ""
                await model.searchTranscripts()
                return
            }
            do {
                try await Task.sleep(nanoseconds: 300_000_000)
            } catch {
                return
            }
            model.debouncedQuery = model.query
            await model.searchTranscripts()
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

        transcriptSection

        if !model.wikiResults.isEmpty {
            Section("Wiki") {
                ForEach(model.wikiResults) { hit in
                    Button {
                        Haptics.selection()
                        destination = .wiki(hit.page)
                    } label: {
                        PodcastWikiSearchRow(hit: hit, query: model.query)
                    }
                    .buttonStyle(.plain)
                }
            }
        }
    }

    @ViewBuilder
    private var transcriptSection: some View {
        if shouldShowTranscriptSection {
            Section("Transcripts") {
                if model.isSearchingTranscripts {
                    ProgressView()
                }
                ForEach(model.transcriptResults) { hit in
                    Button {
                        openTranscriptHit(hit)
                    } label: {
                        PodcastTranscriptSearchRow(
                            hit: hit,
                            episode: store.episode(id: hit.chunk.episodeID),
                            podcast: store.podcast(id: hit.chunk.podcastID),
                            query: model.query
                        )
                    }
                    .buttonStyle(.plain)
                }
                if let error = model.transcriptError {
                    Label(error, systemImage: "exclamationmark.triangle")
                        .font(AppTheme.Typography.caption)
                        .foregroundStyle(.secondary)
                }
            }
        }
    }

    private var resultCount: Int {
        localResults.shows.count
            + localResults.episodes.count
            + model.transcriptResults.count
            + model.wikiResults.count
    }

    private func openTranscriptHit(_ hit: PodcastTranscriptSearchHit) {
        Haptics.selection()
        if let episode = store.episode(id: hit.chunk.episodeID) {
            // Tapping a transcript hit reads as "play this moment" — load
            // the episode, seek to the chunk's start, and start playback.
            // Previously this only set + seeked, leaving the user on a
            // paused episode with no obvious cue why nothing was rolling.
            // setEpisode is idempotent on same-id, so calling it when the
            // episode is already loaded is a no-op and won't re-buffer.
            playback.setEpisode(episode)
            playback.seek(to: Double(hit.chunk.startMS) / 1000)
            if !playback.isPlaying {
                playback.play()
            }
        }
        destination = .episode(hit.chunk.episodeID)
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
        case .wiki(let page):
            WikiPageView(
                page: page,
                storage: model.wikiStorage,
                onDeleted: { id in model.removeWikiPage(id: id) },
                onRegenerated: { page in model.upsertWikiPage(page) }
            )
        }
    }

    private func missingView(_ title: String) -> some View {
        ContentUnavailableView(title, systemImage: "questionmark.folder")
    }
}
