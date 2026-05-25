import SwiftUI

// MARK: - MiniPlayerView
//
// Persistent mini-player bar rendered above the tab bar. Visible when
// `model.podcastSnapshot?.nowPlaying` is non-nil (i.e. an episode is loaded).
//
// Doctrine:
//   D7 — all play/pause decisions are Rust-side. The view only dispatches
//        `podcast.player.play` / `podcast.player.pause` and renders the
//        PlayerState snapshot as-is.
//   D5 — no default-zero renders. When `nowPlaying` is nil the bar is
//        entirely absent from the hierarchy (no placeholder, no skeleton).

struct MiniPlayerView: View {
    @Environment(KernelModel.self) private var model

    @State private var showPlayer = false

    private var nowPlaying: PlayerState? {
        model.podcastSnapshot?.nowPlaying
    }

    /// Find episode metadata by matching `episodeId` against the library.
    private var episode: EpisodeSummary? {
        guard let epId = nowPlaying?.episodeId else { return nil }
        return model.library.flatMap { $0.episodes }.first { $0.id == epId }
    }

    var body: some View {
        if let player = nowPlaying {
            bar(player: player)
                .transition(.move(edge: .bottom).combined(with: .opacity))
                .onTapGesture { showPlayer = true }
                .onReceive(NotificationCenter.default.publisher(for: .openPlayerRequested)) { _ in
                    showPlayer = true
                }
                .fullScreenCover(isPresented: $showPlayer) {
                    PlayerView()
                }
        }
    }

    // MARK: - Bar layout

    private func bar(player: PlayerState) -> some View {
        VStack(spacing: 0) {
            progressBar(player: player)
            HStack(spacing: PodcastSpace.m) {
                artwork
                titleStack(player: player)
                Spacer(minLength: 0)
                controls(player: player)
            }
            .padding(.horizontal, PodcastSpace.l)
            .padding(.vertical, PodcastSpace.s)
        }
        .background(.regularMaterial)
        .clipShape(RoundedRectangle(cornerRadius: PodcastSpace.radiusSmall, style: .continuous))
        .shadow(color: .black.opacity(0.12), radius: 8, y: -2)
        .padding(.horizontal, PodcastSpace.s)
        .padding(.bottom, PodcastSpace.xs)
    }

    // MARK: Progress bar

    private func progressBar(player: PlayerState) -> some View {
        let duration = player.durationSecs ?? 0
        let fraction = (duration > 0) ? min(max(player.positionSecs / duration, 0), 1) : 0
        return GeometryReader { geo in
            ZStack(alignment: .leading) {
                Rectangle()
                    .fill(PodcastColor.hairline)
                    .frame(height: 2)
                Rectangle()
                    .fill(PodcastColor.accent)
                    .frame(width: geo.size.width * fraction, height: 2)
            }
        }
        .frame(height: 2)
    }

    // MARK: Artwork

    private var artwork: some View {
        Group {
            if let urlStr = episode?.artworkUrl,
               let url = URL(string: urlStr) {
                AsyncImage(url: url) { image in
                    image.resizable().scaledToFill()
                } placeholder: {
                    artworkPlaceholder
                }
            } else {
                artworkPlaceholder
            }
        }
        .frame(width: 40, height: 40)
        .clipShape(RoundedRectangle(cornerRadius: PodcastSpace.xs, style: .continuous))
    }

    private var artworkPlaceholder: some View {
        RoundedRectangle(cornerRadius: PodcastSpace.xs, style: .continuous)
            .fill(PodcastColor.surface)
            .overlay {
                Image(systemName: "headphones")
                    .foregroundStyle(PodcastColor.textTertiary)
                    .font(.system(size: 16))
            }
    }

    // MARK: Title stack

    private func titleStack(player: PlayerState) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(episode?.title ?? "Now Playing")
                .font(PodcastFont.callout.weight(.medium))
                .foregroundStyle(PodcastColor.textPrimary)
                .lineLimit(1)
            if let podcastTitle = episode?.podcastTitle {
                Text(podcastTitle)
                    .font(PodcastFont.caption)
                    .foregroundStyle(PodcastColor.textSecondary)
                    .lineLimit(1)
            }
        }
    }

    // MARK: Controls

    private func controls(player: PlayerState) -> some View {
        HStack(spacing: PodcastSpace.s) {
            playPauseButton(isPlaying: player.isPlaying)
            skipForwardButton
        }
    }

    private func playPauseButton(isPlaying: Bool) -> some View {
        Button {
            if isPlaying {
                model.dispatch(namespace: "podcast.player", body: ["op": "pause"])
            } else {
                // Resume: re-dispatch play for the current episodeId (if known).
                if let epId = nowPlaying?.episodeId {
                    model.dispatch(namespace: "podcast.player", body: [
                        "op": "play",
                        "episode_id": epId
                    ])
                }
            }
        } label: {
            Image(systemName: isPlaying ? "pause.fill" : "play.fill")
                .font(.system(size: 22, weight: .semibold))
                .foregroundStyle(PodcastColor.textPrimary)
                .frame(width: 36, height: 36)
        }
        .buttonStyle(.plain)
    }

    private var skipForwardButton: some View {
        Button {
            guard let pos = nowPlaying?.positionSecs else { return }
            model.dispatch(namespace: "podcast.player", body: [
                "op": "seek",
                "position_secs": pos + 30
            ])
        } label: {
            Image(systemName: "goforward.30")
                .font(.system(size: 20, weight: .medium))
                .foregroundStyle(PodcastColor.textPrimary)
                .frame(width: 36, height: 36)
        }
        .buttonStyle(.plain)
    }
}
