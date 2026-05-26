import SwiftUI

/// Flat list of all starred/bookmarked episodes across the library.
/// Episodes are sourced from the kernel snapshot — no local state.
struct StarredEpisodesView: View {
    @Environment(KernelModel.self) private var model

    private var starredEpisodes: [(episode: EpisodeSummary, podcast: PodcastSummary)] {
        model.library.flatMap { podcast in
            podcast.episodes
                .filter(\.starred)
                .map { (episode: $0, podcast: podcast) }
        }
    }

    var body: some View {
        Group {
            if starredEpisodes.isEmpty {
                ContentUnavailableView(
                    "No Bookmarks",
                    systemImage: "bookmark",
                    description: Text("Swipe an episode or tap the bookmark icon to save it here.")
                )
            } else {
                List {
                    ForEach(starredEpisodes, id: \.episode.id) { item in
                        NavigationLink(value: EpisodeRoute(episode: item.episode, podcast: item.podcast)) {
                            starredRow(item.episode, podcast: item.podcast)
                        }
                        .swipeActions(edge: .trailing, allowsFullSwipe: true) {
                            Button(role: .destructive) {
                                model.dispatch(
                                    namespace: "podcast",
                                    body: ["op": "star_episode", "episode_id": item.episode.id,
                                           "starred": false]
                                )
                            } label: {
                                Label("Remove", systemImage: "bookmark.slash")
                            }
                        }
                    }
                }
                .listStyle(.plain)
            }
        }
        .navigationTitle("Bookmarks")
        .navigationBarTitleDisplayMode(.large)
    }

    private func starredRow(_ ep: EpisodeSummary, podcast: PodcastSummary) -> some View {
        HStack(spacing: AppTheme.Spacing.md) {
            artwork(ep: ep, podcast: podcast)

            VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
                Text(ep.title)
                    .font(AppTheme.Typography.headline)
                    .foregroundStyle(ep.played ? Color.secondary : Color.primary)
                    .lineLimit(2)

                Text(ep.podcastTitle ?? podcast.title)
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)

                if let secs = ep.durationSecs {
                    Text(formatDuration(secs))
                        .font(AppTheme.Typography.monoCaption)
                        .foregroundStyle(.tertiary)
                }
            }

            Spacer()

            Image(systemName: "bookmark.fill")
                .font(.caption)
                .foregroundStyle(.orange)
        }
        .padding(.vertical, AppTheme.Spacing.xs)
    }

    private func artwork(ep: EpisodeSummary, podcast: PodcastSummary) -> some View {
        let urlStr = ep.artworkUrl ?? podcast.artworkUrl
        let shape = RoundedRectangle(cornerRadius: AppTheme.Corner.sm, style: .continuous)
        return Group {
            if let urlStr, let url = URL(string: urlStr) {
                AsyncImage(url: url) { phase in
                    switch phase {
                    case .success(let image): image.resizable().scaledToFill()
                    default: artworkPlaceholder
                    }
                }
            } else {
                artworkPlaceholder
            }
        }
        .frame(width: 48, height: 48)
        .clipShape(shape)
        .accessibilityHidden(true)
    }

    private var artworkPlaceholder: some View {
        ZStack {
            Color.secondary.opacity(0.18)
            Image(systemName: "waveform")
                .font(.system(size: 16, weight: .light))
                .foregroundStyle(.secondary)
        }
    }

}
