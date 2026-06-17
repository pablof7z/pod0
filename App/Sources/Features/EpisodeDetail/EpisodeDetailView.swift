import SwiftUI

// MARK: - EpisodeDetailView

/// Episode Detail surface. Single-mode magazine cover: artwork hero, summary
/// lede, chapters (publisher or AI-synthesised), show notes, and a floating
/// global mini player.
///
/// Transcripts are an internal extraction layer — they feed RAG, clip
/// selection, ad detection, and the agent's tools — but they are never the
/// primary "what's playing now" reading surface. Background ingest still
/// starts here for idle episodes, but Rust decides whether that means
/// publisher fetch, STT, or skip; the transcript text itself stays out of
/// sight.
///
/// Driven by the real `Episode` looked up out of `AppStateStore` via the
/// passed `episodeID`. On first appearance for an episode with a `.none`
/// transcript state, we kick off a background `TranscriptIngestService` warm
/// so RAG / agent paths fill in without blocking the user surface.
struct EpisodeDetailView: View {

    // MARK: Inputs

    let episodeID: UUID

    // MARK: Environment

    @Environment(AppStateStore.self) private var store
    @Environment(PlaybackState.self) private var playback

    // MARK: State

    // MARK: Body

    var body: some View {
        Group {
            if let episode = store.episode(id: episodeID) {
                loaded(episode: episode)
            } else {
                missing
            }
        }
        .background(Color(.systemBackground).ignoresSafeArea())
    }

    // MARK: - Loaded

    @ViewBuilder
    private func loaded(episode: Episode) -> some View {
        let subscription = store.podcast(id: episode.podcastID)
        let showName = subscription?.title ?? "Podcast"
        let showImageURL = subscription?.imageURL

        // No inline player chrome — the global `MiniPlayerView` lives as
        // the tab's bottom accessory and is always visible while an episode
        // is loaded.
        // When this episode is currently active in the player (playing or paused),
        // use the engine's live position for the Play/Resume decision rather than
        // the debounce-cached store value, which may not have flushed yet.
        let displayEpisode: Episode = {
            guard playback.episode?.id == episode.id, playback.currentTime > 0 else {
                return episode
            }
            var copy = episode
            copy.playbackPosition = max(episode.playbackPosition, playback.currentTime)
            return copy
        }()
        EpisodeDetailHeroView(
            episode: displayEpisode,
            showName: showName,
            showImageURL: showImageURL,
            isPlayed: episode.played,
            onPlay: {
                playback.setEpisode(episode)
                playback.play()
            },
            onPlayChapter: { chapter in
                if playback.episode?.id != episode.id {
                    playback.setEpisode(episode)
                }
                playback.seek(to: chapter.startTime)
                if !playback.isPlaying {
                    playback.play()
                }
            },
            isInQueue: playback.isQueued(episode.id),
            onAddToQueue: {
                Haptics.success()
                playback.enqueue(episode.id)
            },
            activeChapterID: liveActiveChapterID(for: episode),
            onToggleDownload: { toggleDownload(episode: episode) }
        )
        .navigationTitle(showName)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar { actionsToolbar(episode: episode) }
        .task(id: episode.id) {
            await warmTranscriptIfNeeded(episode: episode)
            ChaptersHydrationService.shared.hydrateIfNeeded(
                episode: episode,
                store: store
            )
            // Compile chapters + summaries + ad segments via the kernel (D0).
            // Idempotent: the kernel gates on whether ad detection has already run.
            store.kernelCompileChapters(episodeID: episode.id)
        }
    }

    /// Warm the transcript on first appearance. Rust owns whether the episode
    /// should fetch a publisher transcript, run STT, or skip; Swift only avoids
    /// re-entering when the projected state is already non-idle.
    ///
    /// We deliberately do not retry `.failed` here — failures sit until
    /// the user re-arms ingestion via Settings → Transcripts.
    private func warmTranscriptIfNeeded(episode: Episode) async {
        guard case .none = episode.transcriptState else { return }
        await TranscriptIngestService.shared.ingest(episodeID: episode.id)
    }

    // MARK: - Missing

    private var missing: some View {
        ContentUnavailableView(
            "Episode not found",
            systemImage: "questionmark.folder",
            description: Text("This episode is no longer in your library.")
        )
    }

    // MARK: - Helpers

    private func navigableChapters(for episode: Episode) -> [Episode.Chapter]? {
        episode.chapters?.filter(\.includeInTableOfContents)
    }

    private func activeChapterID(in chapters: [Episode.Chapter]) -> UUID? {
        chapters.active(at: playback.currentTime)?.id
    }

    /// Active chapter id when this exact episode is currently loaded in
    /// the player. Returns `nil` for chapter-less episodes or when
    /// playback is on a different episode — the hero's chapter list
    /// renders flat in those cases.
    private func liveActiveChapterID(for episode: Episode) -> UUID? {
        guard playback.episode?.id == episode.id,
              let chapters = navigableChapters(for: episode),
              !chapters.isEmpty else { return nil }
        return activeChapterID(in: chapters)
    }

    /// Resolve the persisted `Transcript` for `episode` when its lifecycle is
    /// `.ready`. Kept as a thin static helper because tests pin its behaviour
    /// — see `EpisodeDetailTranscriptTests`. The transcript itself is no
    /// longer rendered as a primary surface here; it remains the extraction
    /// substrate for RAG, clip composer, and the agent's tool layer.
    static func readyTranscript(
        for episode: Episode,
        store: TranscriptStore = .shared
    ) -> Transcript? {
        guard case .ready = episode.transcriptState else { return nil }
        return store.load(episodeID: episode.id)
    }

    /// Drives the inline Download pill on the hero. Mirrors the menu's
    /// state machine so the user can start, cancel, or retry from the
    /// primary surface — and sees a live "Downloading 42%" badge while
    /// bytes move.
    private func toggleDownload(episode: Episode) {
        switch episode.downloadState {
        case .notDownloaded, .queued, .failed:
            Haptics.success()
            store.kernelDownload(episode.id)
        case .downloading:
            Haptics.light()
            store.kernelCancelDownload(episode.id)
        case .downloaded:
            // Inline pill is non-interactive in the downloaded state; the
            // ellipsis menu handles delete confirmation.
            break
        }
    }

    @ToolbarContentBuilder
    private func actionsToolbar(episode: Episode) -> some ToolbarContent {
        if case .downloading(let progress, _) = episode.downloadState {
            ToolbarItem(placement: .topBarTrailing) {
                ProgressView(value: progress.clamped01)
                    .progressViewStyle(.circular)
                    .controlSize(.small)
                    .accessibilityLabel("Downloading \(Int(progress.clamped01 * 100)) percent")
            }
        }
        ToolbarItem(placement: .topBarTrailing) {
            EpisodeDetailActionsMenu(episode: episode, store: store)
        }
    }
}

// MARK: - Preview

#Preview("Detail") {
    let store = AppStateStore()
    let playback = PlaybackState()
    let subID = UUID()
    let podcast = Podcast(
        id: subID,
        feedURL: URL(string: "https://feeds.megaphone.fm/tim-ferriss")!,
        title: "The Tim Ferriss Show",
        author: "Tim Ferriss",
        description: "Deconstructing world-class performers."
    )
    let episode = Episode(
        podcastID: subID,
        guid: "preview-tim-ferriss-732",
        title: "How to Think About Keto",
        description: "<p>Tim sits down with <b>Peter Attia, MD</b> to revisit a topic the show has circled for years.</p>",
        pubDate: Date(timeIntervalSince1970: 1_714_780_800),
        duration: 60 * 60 * 2 + 14 * 60,
        enclosureURL: URL(string: "https://traffic.megaphone.fm/HSW1234567890.mp3")!,
        chapters: [
            .init(startTime: 0, title: "Cold open"),
            .init(startTime: 252, title: "Why ketones matter"),
            .init(startTime: 1720, title: "The Inuit objection"),
            .init(startTime: 4810, title: "Practical protocols")
        ]
    )
    store.state.podcasts = [podcast]
    store.state.subscriptions = [PodcastSubscription(podcastID: subID)]
    store.episodes = [episode]
    return NavigationStack {
        EpisodeDetailView(episodeID: episode.id)
    }
    .environment(store)
    .environment(playback)
}
