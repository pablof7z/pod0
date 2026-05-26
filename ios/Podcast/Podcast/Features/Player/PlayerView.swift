import SwiftUI

// MARK: - PlayerView
//
// Full-screen now-playing surface. Presented as a `.fullScreenCover` from the
// mini-player (or from any view posting `.openPlayerRequested`).
//
// Layout:
//   - Background: blurred + darkened artwork
//   - Top: chevron-down close button
//   - Center: square artwork card, episode title, podcast name
//   - Bottom glass island (PlayerControls): scrubber, transport, speed/sleep/AirPlay
//
// Doctrine: every read comes from `model.podcastSnapshot?.nowPlaying`.
// Episode + podcast metadata are resolved by scanning the library; no
// derived state, no caches.

struct PlayerView: View {
    @Environment(KernelModel.self) private var model
    @Environment(\.dismiss) private var dismiss

    @State private var scrubbingPosition: Double?
    @State private var showSpeedSheet = false
    @State private var showSleepSheet = false
    @State private var showUpNextSheet = false
    @State private var showClipComposer = false

    private var nowPlaying: PlayerState? {
        model.podcastSnapshot?.nowPlaying
    }

    private var episode: EpisodeSummary? {
        guard let epId = nowPlaying?.episodeId else { return nil }
        return model.library.flatMap { $0.episodes }.first { $0.id == epId }
    }

    private var podcastTitle: String? {
        guard let epId = nowPlaying?.episodeId else { return nil }
        return model.library.first { show in
            show.episodes.contains { $0.id == epId }
        }?.title
    }

    private var artworkUrl: URL? {
        guard let str = episode?.artworkUrl, let url = URL(string: str) else { return nil }
        return url
    }

    var body: some View {
        ZStack {
            background
            content
        }
        .preferredColorScheme(.dark)
        .sheet(isPresented: $showUpNextSheet) {
            UpNextSheet()
        }
    }

    // MARK: - Background

    @ViewBuilder
    private var background: some View {
        if let url = artworkUrl {
            AsyncImage(url: url) { phase in
                switch phase {
                case .success(let image):
                    image
                        .resizable()
                        .scaledToFill()
                        .blur(radius: 60, opaque: true)
                        .overlay(Color.black.opacity(0.55))
                default:
                    fallbackBackground
                }
            }
            .ignoresSafeArea()
        } else {
            fallbackBackground
                .ignoresSafeArea()
        }
    }

    private var fallbackBackground: some View {
        LinearGradient(
            colors: [Color.black, Color(white: 0.15)],
            startPoint: .top,
            endPoint: .bottom
        )
    }

    // MARK: - Foreground content

    @ViewBuilder
    private var content: some View {
        if let player = nowPlaying {
            VStack(spacing: 0) {
                topBar
                Spacer(minLength: PodcastSpace.m)
                artworkCard
                Spacer(minLength: PodcastSpace.l)
                metadata
                Spacer(minLength: PodcastSpace.m)
                chapterRail(player: player)
                Spacer(minLength: PodcastSpace.m)
                PlayerControls(
                    player: player,
                    scrubbingPosition: $scrubbingPosition,
                    showSpeedSheet: $showSpeedSheet,
                    showSleepSheet: $showSleepSheet,
                    showClipComposer: $showClipComposer
                )
                .padding(.horizontal, PodcastSpace.l)
                .padding(.bottom, PodcastSpace.l)
            }
            .task { dispatchFetchChaptersIfNeeded(for: player.episodeId) }
            .onChange(of: player.episodeId) { _, newId in
                dispatchFetchChaptersIfNeeded(for: newId)
            }
            .sheet(isPresented: $showClipComposer) {
                ClipComposerView(
                    player: player,
                    episode: episode,
                    podcastTitle: podcastTitle
                )
                .environment(model)
            }
        } else {
            emptyState
        }
    }

    // MARK: - Chapter rail

    @ViewBuilder
    private func chapterRail(player: PlayerState) -> some View {
        if let chapters = episode?.chapters, !chapters.isEmpty {
            ChapterRailView(
                chapters: chapters,
                currentPositionSecs: scrubbingPosition ?? player.positionSecs,
                onSeek: { seconds in
                    model.dispatch(namespace: "podcast.player", body: [
                        "op": "seek",
                        "position_secs": seconds
                    ])
                }
            )
        }
    }

    private func dispatchFetchChaptersIfNeeded(for episodeId: String?) {
        guard let episodeId else { return }
        // Self-gating lives in Rust (`handle_fetch_chapters` short-circuits on
        // missing chapters_url or already-loaded chapters). The view fires the
        // dispatch unconditionally on every episode change — D7, no policy in
        // the shell.
        model.dispatch(namespace: "podcast", body: [
            "op": "fetch_chapters",
            "episode_id": episodeId
        ])
    }

    // MARK: - Top bar

    private var topBar: some View {
        HStack {
            Button {
                dismiss()
            } label: {
                Image(systemName: "chevron.down")
                    .font(.system(size: 20, weight: .semibold))
                    .foregroundStyle(.white)
                    .frame(width: 44, height: 44)
                    .background(.ultraThinMaterial, in: Circle())
                    .contentShape(Circle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Close player")
            Spacer()
            Button {
                showUpNextSheet = true
            } label: {
                Image(systemName: "list.bullet")
                    .font(.system(size: 20, weight: .semibold))
                    .foregroundStyle(.white)
                    .frame(width: 44, height: 44)
                    .background(.ultraThinMaterial, in: Circle())
                    .contentShape(Circle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Up Next queue")
        }
        .padding(.horizontal, PodcastSpace.l)
        .padding(.top, PodcastSpace.s)
    }

    // MARK: - Artwork card

    private var artworkCard: some View {
        GeometryReader { geo in
            let side = min(geo.size.width, geo.size.height) - PodcastSpace.xl * 2
            HStack {
                Spacer()
                Group {
                    if let url = artworkUrl {
                        AsyncImage(url: url) { image in
                            image.resizable().scaledToFill()
                        } placeholder: {
                            artworkPlaceholder
                        }
                    } else {
                        artworkPlaceholder
                    }
                }
                .frame(width: side, height: side)
                .clipShape(RoundedRectangle(cornerRadius: PodcastSpace.radius, style: .continuous))
                .shadow(color: .black.opacity(0.5), radius: 30, y: 12)
                Spacer()
            }
            .frame(maxHeight: .infinity, alignment: .center)
        }
        .frame(minHeight: 240)
    }

    private var artworkPlaceholder: some View {
        ZStack {
            PodcastColor.surface
            Image(systemName: "headphones")
                .font(.system(size: 60, weight: .light))
                .foregroundStyle(PodcastColor.textTertiary)
        }
    }

    // MARK: - Metadata

    private var metadata: some View {
        HStack(alignment: .center, spacing: PodcastSpace.m) {
            Spacer()
            VStack(spacing: PodcastSpace.xs) {
                Text(episode?.title ?? "Now Playing")
                    .font(PodcastFont.title)
                    .foregroundStyle(.white)
                    .multilineTextAlignment(.center)
                    .lineLimit(2)
                if let title = podcastTitle ?? episode?.podcastTitle {
                    Text(title)
                        .font(PodcastFont.callout)
                        .foregroundStyle(Color.white.opacity(0.75))
                        .lineLimit(1)
                }
            }
            .frame(maxWidth: .infinity)

            Button {
                guard let ep = episode else { return }
                Haptics.light()
                model.dispatch(
                    namespace: "podcast",
                    body: ["op": "star_episode", "episode_id": ep.id]
                )
            } label: {
                Image(systemName: episode?.starred == true ? "bookmark.fill" : "bookmark")
                    .font(.system(size: 20, weight: .regular))
                    .foregroundStyle(.white)
                    .frame(width: 44, height: 44)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel(episode?.starred == true ? "Remove bookmark" : "Bookmark episode")
        }
        .padding(.horizontal, PodcastSpace.l)
    }

    // MARK: - Empty state

    private var emptyState: some View {
        VStack(spacing: PodcastSpace.l) {
            topBar
            Spacer()
            PodcastPlaceholder(
                systemImage: "play.slash",
                title: "Nothing playing",
                subtitle: "Pick an episode from your library to start."
            )
            .foregroundStyle(.white)
            Spacer()
        }
    }
}
