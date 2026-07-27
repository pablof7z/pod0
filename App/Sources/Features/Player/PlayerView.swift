import SwiftUI

/// Full-screen Now Playing surface.
///
/// The top bar floats above a paged chapters/show-notes surface. The playback
/// transport and action cluster floats at the bottom without a visual container
/// via `safeAreaInset(edge: .bottom)`. Colors and fonts
/// use semantic / Dynamic Type styles so the surface adapts to the user's
/// appearance settings and accent color.
struct PlayerView: View {
    @Environment(AppStateStore.self) private var store
    @Environment(WorkflowClient.self) private var workflows
    @Bindable var state: PlaybackState
    @Environment(\.dismiss) private var dismiss
    let glassNamespace: Namespace.ID
    @State private var showSpeedSheet: Bool = false
    @State private var showSleepSheet: Bool = false
    @State private var showShareSheet: Bool = false
    @State private var showVoiceNoteSheet: Bool = false
    @State private var showingShowNotes: Bool = false
    @State private var episodeDetailTarget: UUID? = nil
    private var podcast: Podcast? {
        guard let podID = state.episode?.podcastID else { return nil }
        return store.podcast(id: podID)
    }

    private var showName: String {
        podcast?.title ?? ""
    }
    var body: some View {
        VStack(spacing: 0) {
            episodeHeader
                .padding(.horizontal, AppTheme.Spacing.md)
                .padding(.top, AppTheme.Spacing.sm)
            PlayerEpisodeProgressView(state: state)
                .padding(.horizontal, AppTheme.Spacing.md)
            carouselPageIndicator
                .padding(.horizontal, AppTheme.Spacing.md)
            TabView(selection: $showingShowNotes) {
                ScrollViewReader { proxy in
                    ScrollView(.vertical, showsIndicators: false) {
                        chaptersPanel
                            .padding(.horizontal, AppTheme.Spacing.md)
                            .padding(.bottom, AppTheme.Spacing.lg)
                    }
                    .onAppear {
                        guard let activeID = navigableChapters?.active(at: state.currentTime)?.id else { return }
                        proxy.scrollTo(activeID, anchor: .center)
                    }
                }
                .tag(false)
                ScrollView(.vertical, showsIndicators: false) {
                    PlayerShowNotesView(episode: liveEpisode)
                        .padding(.horizontal, AppTheme.Spacing.md)
                        .padding(.bottom, AppTheme.Spacing.lg)
                }
                .tag(true)
            }
            .tabViewStyle(.page(indexDisplayMode: .never))
        }
        .safeAreaInset(edge: .top, spacing: 0) { topBar }
        .safeAreaInset(edge: .bottom, spacing: 0) { floatingChrome }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background {
            PlayerEditorialBackdrop(artworkURL: artworkURL)
        }
        .sheet(isPresented: $showSpeedSheet) { PlayerSpeedSheet(state: state) }
        .sheet(isPresented: $showSleepSheet) { PlayerSleepTimerSheet(state: state) }
        .sheet(isPresented: $showVoiceNoteSheet) {
            VoiceNoteRecordingSheet(state: state)
                .environment(store)
        }
        .sheet(isPresented: $showShareSheet) {
            if let episode = state.episode {
                PlayerShareSheet(state: state, episode: episode, showName: showName)
            }
        }
        .sheet(item: Binding(
            get: { episodeDetailTarget.map(EpisodeDetailTarget.init) },
            set: { episodeDetailTarget = $0?.id }
        )) { target in
            NavigationStack {
                EpisodeDetailView(episodeID: target.id)
            }
            .environment(state)
        }
        .onReceive(NotificationCenter.default.publisher(for: .openEpisodeDetailRequested)) { note in
            guard let idString = note.userInfo?["episodeID"] as? String,
                  let uuid = UUID(uuidString: idString) else { return }
            episodeDetailTarget = uuid
        }
        .task(id: state.episode?.id) {
            if state.episode != nil { workflows.wake() }
            AutoSnipController.shared.attach(playback: state, store: store)
        }
        .autoSnipPresentation(controller: AutoSnipController.shared)
        .workflowProjectionScope(
            subjectIDs: state.episode.map { [$0.id] } ?? [],
            kinds: [.transcriptIngest, .chapterArtifacts]
        )
    }

    // MARK: - Top bar

    private var topBar: some View {
        PlayerTopBar(
            state: state,
            podcast: podcast,
            showName: showName,
            artworkURL: artworkURL,
            titleCollapsed: false,
            onDismiss: { dismiss() },
            onShare: { showShareSheet = true },
            onShowSleepTimer: { showSleepSheet = true },
            onShowSpeed: { showSpeedSheet = true }
        )
    }

    // MARK: - Episode header (compact: artwork left, text right)

    /// Resolved artwork URL with per-chapter override. Priority:
    ///   1. Active chapter's `imageURL`
    ///   2. Per-episode artwork (`<itunes:image>` override)
    ///   3. Show-level cover art via `PlaybackState.resolveShowImage`
    private var artworkURL: URL? {
        guard let episode = state.episode else { return nil }
        if let chapterImage = activeChapterImageURL { return chapterImage }
        return episode.imageURL ?? state.resolveShowImage(episode)
    }

    private var activeChapterImageURL: URL? {
        guard let chapters = navigableChapters, !chapters.isEmpty else { return nil }
        return chapters.active(at: state.currentTime)?.imageURL
    }

    private var activeChapterSourceEpisodeID: String? {
        guard let chapters = navigableChapters, !chapters.isEmpty else { return nil }
        return chapters.active(at: state.currentTime)?.sourceEpisodeID
    }

    private var episodeHeader: some View {
        HStack(alignment: .top, spacing: AppTheme.Spacing.md) {
            compactArtwork
            if let episode = state.episode {
                VStack(alignment: .leading, spacing: 6) {
                    Button {
                        Haptics.selection()
                        episodeDetailTarget = episode.id
                    } label: {
                        Text(episode.title)
                            .font(AppTheme.Typography.title)
                            .foregroundStyle(.primary)
                            .fixedSize(horizontal: false, vertical: true)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityHint("Opens episode details")
                    if !showName.isEmpty {
                        Text(showName)
                            .font(.system(.subheadline, design: .rounded).weight(.medium))
                            .foregroundStyle(.secondary)
                    }
                    if !metadataLine.isEmpty {
                        Text(metadataLine)
                            .font(.caption)
                            .foregroundStyle(.tertiary)
                    }
                    generationSourceChip
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var compactArtwork: some View {
        ZStack {
            if let url = artworkURL {
                CachedAsyncImage(url: url, targetSize: CGSize(width: 220, height: 220)) { phase in
                    switch phase {
                    case .success(let image):
                        image.resizable().scaledToFill()
                    default:
                        compactArtworkFallback
                    }
                }
                .id(url)
                .transition(.opacity)
            } else {
                compactArtworkFallback
            }
        }
        .frame(width: 110, height: 110)
        .clipShape(RoundedRectangle(cornerRadius: AppTheme.Corner.lg, style: .continuous))
        .glassEffectID("player.artwork", in: glassNamespace)
        .animation(.easeInOut(duration: 0.35), value: artworkURL)
        .accessibilityHidden(true)
    }

    private var compactArtworkFallback: some View {
        ZStack {
            Color.secondary.opacity(0.18)
            Image(systemName: "waveform")
                .font(.system(size: 28, weight: .light))
                .foregroundStyle(.secondary)
        }
    }

    private var metadataLine: String {
        guard let episode = state.episode else { return "" }
        let f = DateFormatter()
        f.dateFormat = "MMM d, yyyy"
        let date = f.string(from: episode.pubDate)
        if let duration = episode.duration {
            let mins = Int(duration / 60)
            let h = mins / 60
            let m = mins % 60
            let durString = h > 0 ? "\(h)h \(m)m" : "\(m)m"
            return "\(date) · \(durString)"
        }
        return date
    }

    // MARK: - Carousel page indicator

    private var carouselPageIndicator: some View {
        HStack(spacing: 5) {
            Capsule()
                .fill(!showingShowNotes ? Color.primary.opacity(0.7) : Color.secondary.opacity(0.25))
                .frame(width: !showingShowNotes ? 16 : 6, height: 5)
            Capsule()
                .fill(showingShowNotes ? Color.primary.opacity(0.7) : Color.secondary.opacity(0.25))
                .frame(width: showingShowNotes ? 16 : 6, height: 5)
        }
        .frame(maxWidth: .infinity)
        .animation(AppTheme.Animation.spring, value: showingShowNotes)
        .padding(.bottom, 2)
    }

    @ViewBuilder
    private var chaptersPanel: some View {
        if let chapters = navigableChapters, !chapters.isEmpty {
            PlayerChaptersScrollView(
                chapters: chapters,
                notes: episodeNotes,
                state: state
            )
        } else {
            PlayerNoChaptersPlaceholder(episode: liveEpisode)
        }
    }

    /// Episode-anchored notes for the currently-playing episode, fed into the
    /// chapter rail for chronological interleaving.
    private var episodeNotes: [Note] {
        guard let id = state.episode?.id else { return [] }
        return store.notes(forEpisode: id)
    }

    private var liveEpisode: Episode? {
        guard let id = state.episode?.id else { return nil }
        return store.episode(id: id) ?? state.episode
    }

    private var navigableChapters: [Episode.Chapter]? {
        let liveEpisode = state.episode.flatMap { store.episode(id: $0.id) } ?? state.episode
        return liveEpisode?.chapters?.filter(\.includeInTableOfContents)
    }

    // MARK: - Generation source chip

    @ViewBuilder
    private var generationSourceChip: some View {
        let resolved = state.episode.flatMap { store.episode(id: $0.id) } ?? state.episode
        if let source = resolved?.generationSource {
            PlayerGenerationSourceChip(source: source)
                .animation(.easeInOut(duration: 0.25), value: true)
        }
    }

    // MARK: - Floating playback chrome

    private var floatingChrome: some View {
        PlayerPlaybackChrome(
            state: state,
            sourceEpisodeID: activeChapterSourceEpisodeID,
            episode: liveEpisode,
            chapters: navigableChapters ?? [],
            showVoiceNoteSheet: $showVoiceNoteSheet
        )
    }

    private struct EpisodeDetailTarget: Identifiable {
        let id: UUID
    }
}
