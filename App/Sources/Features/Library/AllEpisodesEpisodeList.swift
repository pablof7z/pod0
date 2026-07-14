import SwiftUI

// MARK: - AllEpisodesEpisodeList

/// Episode list for `AllEpisodesView`. Mirrors the structure of
/// `ShowDetailEpisodeList` but resolves each episode's parent podcast from
/// a pre-built dictionary and bumps `visibleCount` when the last visible row
/// appears — the mechanism that drives scroll-triggered pagination.
struct AllEpisodesEpisodeList: View {
    let episodes: [Episode]
    let podcastsByID: [UUID: Podcast]
    @Binding var voiceOverDetailRoute: LibraryEpisodeRoute?
    @Binding var visibleCount: Int
    let totalCount: Int

    @Environment(AppStateStore.self) private var store
    @Environment(PlaybackState.self) private var playback

    var body: some View {
        ForEach(episodes) { ep in
            let podcast = podcastsByID[ep.podcastID]
            let route = LibraryEpisodeRoute(
                episodeID: ep.id,
                subscriptionID: ep.podcastID,
                title: ep.title
            )
            NavigationLink(value: route) {
                EpisodeRow(
                    episode: ep,
                    showAccent: podcast?.accentColor ?? Color.accentColor,
                    fallbackImageURL: podcast?.imageURL,
                    podcastTitle: podcast?.title,
                    onPlay: {
                        playback.setEpisode(ep)
                        playback.play()
                        NotificationCenter.default.post(name: .openPlayerRequested, object: nil)
                    }
                )
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .listRowSeparator(.hidden)
            .listRowInsets(EdgeInsets(
                top: AppTheme.Spacing.xs,
                leading: AppTheme.Spacing.lg,
                bottom: AppTheme.Spacing.xs,
                trailing: AppTheme.Spacing.lg
            ))
            .listRowBackground(Color(.systemBackground))
            .swipeActions(edge: .leading, allowsFullSwipe: true) {
                EpisodeRowLeadingSwipeAction(episode: ep, playback: playback)
            }
            .swipeActions(edge: .trailing, allowsFullSwipe: false) {
                EpisodeRowTrailingSwipeAction(episode: ep, store: store)
                EpisodeRowDownloadSwipeAction(episode: ep, store: store)
            }
            .contextMenu {
                EpisodeRowContextMenu(
                    episode: ep,
                    store: store,
                    openDetailsRoute: route,
                    playback: playback
                )
            }
            .accessibilityActions {
                EpisodeRowAccessibilityActions(
                    episode: ep,
                    store: store,
                    onOpenDetails: { voiceOverDetailRoute = route }
                )
            }
            .onAppear {
                if ep.id == episodes.last?.id, visibleCount < totalCount {
                    visibleCount += 50
                }
            }
        }
    }
}
