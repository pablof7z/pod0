import AVKit
import SwiftUI

/// Full-screen Now Playing surface.
///
/// Layout: a single vertical `ScrollView` whose content is the compact
/// episode header (artwork left, title/show/metadata right) followed by the
/// chapter rail. The top bar floats with the close button + the
/// share/AirPlay/more cluster, swapping the middle label from show-name to a
/// compact artwork+title once the hero has scrolled offscreen. The playback
/// chrome (scrubber + transport + action cluster) floats at the bottom in a
/// single glass island via `safeAreaInset(edge: .bottom)`. Colors and fonts
/// use semantic / Dynamic Type styles so the surface adapts to the user's
/// appearance settings and accent color.
struct PlayerView: View {

    @Environment(AppStateStore.self) private var store
    @Bindable var state: PlaybackState
    @Environment(\.dismiss) private var dismiss
    let glassNamespace: Namespace.ID

    @State private var isScrubbing: Bool = false
    @State private var showSpeedSheet: Bool = false
    @State private var showSleepSheet: Bool = false
    @State private var showShareSheet: Bool = false
    @State private var showVoiceNoteSheet: Bool = false
    @State private var showingShowNotes: Bool = false
    @State private var episodeDetailTarget: UUID? = nil
    /// Tracks the playhead position captured at the moment the "Add Note"
    /// button was tapped — used as the anchor position for the new note.
    @State private var showAddNoteSheet: Bool = false
    @State private var noteAnchorTime: TimeInterval = 0

    /// Observed so the editorial-header download badge tracks the service's
    /// `progress[id]` map at 5%/200ms without each tick re-rendering through
    /// `AppStateStore`. Mirrors the pattern used by `EpisodeRow`.
    @State private var downloadService = EpisodeDownloadService.shared

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
                .padding(.bottom, AppTheme.Spacing.sm)
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
        .sheet(isPresented: $showAddNoteSheet) {
            if let episode = state.episode {
                let capturedEpisodeID = episode.id
                let capturedTime = noteAnchorTime
                EditTextSheet(title: "Add Note", initialText: "") { text in
                    store.addNote(
                        text: text,
                        kind: .free,
                        target: .episode(id: capturedEpisodeID, positionSeconds: capturedTime)
                    )
                    Haptics.success()
                }
            }
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
            if let episode = state.episode {
                ChaptersHydrationService.shared.hydrateIfNeeded(
                    episode: episode,
                    store: store
                )
                await AIChapterCompiler.shared.compileIfNeeded(
                    episodeID: episode.id,
                    store: store
                )
            }
            AutoSnipController.shared.attach(playback: state, store: store)
        }
        .overlay(alignment: .top) {
            VStack(spacing: AppTheme.Spacing.sm) {
                AutoSnipBanner(controller: AutoSnipController.shared)
                    .allowsHitTesting(false)
                NoLLMKeyHintBanner(controller: AutoSnipController.shared)
            }
            .padding(.top, AppTheme.Spacing.lg)
        }
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
                        openEpisodeDetail(episode)
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
        .blur(radius: isScrubbing ? 4 : 0)
        .glassEffectID("player.artwork", in: glassNamespace)
        .animation(AppTheme.Animation.spring, value: isScrubbing)
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
        HStack(spacing: 0) {
            Spacer()
            HStack(spacing: 5) {
                Capsule()
                    .fill(!showingShowNotes ? Color.primary.opacity(0.7) : Color.secondary.opacity(0.25))
                    .frame(width: !showingShowNotes ? 16 : 6, height: 5)
                Capsule()
                    .fill(showingShowNotes ? Color.primary.opacity(0.7) : Color.secondary.opacity(0.25))
                    .frame(width: showingShowNotes ? 16 : 6, height: 5)
            }
            .animation(AppTheme.Animation.spring, value: showingShowNotes)
            Spacer()
            if !showingShowNotes, state.episode != nil {
                Button {
                    noteAnchorTime = state.currentTime
                    showAddNoteSheet = true
                    Haptics.selection()
                } label: {
                    Image(systemName: "note.text.badge.plus")
                        .font(.footnote.weight(.semibold))
                        .foregroundStyle(.secondary)
                        .frame(width: 28, height: 28)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Add note at current position")
            } else {
                Color.clear.frame(width: 28, height: 28)
            }
        }
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

    // MARK: - Download fraction (for scrubber shade)

    private var downloadFraction: Double? {
        guard let id = state.episode?.id,
              let episode = store.episode(id: id) ?? state.episode else { return nil }
        switch episode.downloadState {
        case .downloading(let persisted, _):
            return (downloadService.progress[id] ?? persisted).clamped01
        case .downloaded:
            return 1.0
        default:
            return nil
        }
    }

    // MARK: - Navigation

    private func openEpisodeDetail(_ episode: Episode) {
        episodeDetailTarget = episode.id
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

    // MARK: - Route picker

    private var routePicker: some View {
        ZStack {
            Image(systemName: "airplayaudio")
                .font(.body.weight(.semibold))
                .foregroundStyle(.primary)
                .frame(width: 44, height: 44)
                .contentShape(Circle())
                .glassEffect(.regular.interactive(), in: .circle)
                .accessibilityHidden(true)
            RoutePickerView(activeTintColor: .clear, tintColor: .clear)
                .allowsHitTesting(true)
                .accessibilityHidden(true)
        }
        .frame(width: 44, height: 44)
        .accessibilityElement(children: .combine)
        .accessibilityLabel("Audio output")
        .accessibilityHint("Opens system output picker")
    }

    // MARK: - Floating playback chrome (scrubber + transport + actions)

    private var episodeClips: [Clip] {
        guard let id = state.episode?.id else { return [] }
        return store.clips(forEpisode: id)
    }

    /// Pulled out of the scroll body and attached via
    /// `safeAreaInset(edge: .bottom)` so the chapter list scrolls under it.
    /// The outer `RoundedRectangle` glass surface gives the chrome a solid
    /// liquid-glass backdrop so chapters scrolling behind it don't bleed
    /// through. The inner `GlassEffectContainer` connects the individual glass
    /// buttons so they morph together on press.
    private var floatingChrome: some View {
        GlassEffectContainer(spacing: AppTheme.Spacing.md) {
            VStack(spacing: AppTheme.Spacing.md) {
                if let sourceID = activeChapterSourceEpisodeID {
                    PlayerClipSourceChip(sourceEpisodeID: sourceID)
                        .animation(.easeInOut(duration: 0.25), value: sourceID)
                }
                PlayerPrerollSkipButton(state: state, episode: liveEpisode)
                    .animation(AppTheme.Animation.spring, value: state.currentTime)
                HStack(alignment: .center, spacing: AppTheme.Spacing.sm) {
                    PlayerScrubberView(
                        state: state,
                        isScrubbing: $isScrubbing,
                        chapters: navigableChapters ?? [],
                        clips: episodeClips,
                        onClipTap: { clip in state.navigationalSeek(to: clip.startSeconds) },
                        downloadFraction: downloadFraction
                    )
                    routePicker
                }
                PlayerControlsView(
                    state: state,
                    glassNamespace: glassNamespace,
                    chapters: navigableChapters ?? [],
                    showVoiceNoteSheet: $showVoiceNoteSheet
                )
            }
            .padding(.horizontal, AppTheme.Spacing.md)
            .padding(.vertical, AppTheme.Spacing.md)
            .glassSurface(cornerRadius: AppTheme.Corner.xl)
        }
        .padding(.horizontal, AppTheme.Spacing.md)
        .padding(.bottom, AppTheme.Spacing.md)
    }

    private struct EpisodeDetailTarget: Identifiable {
        let id: UUID
    }
}
