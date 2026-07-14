import SwiftUI

// MARK: - ContinueListeningView

/// Full-screen list of all in-progress episodes from the last 2 weeks.
/// Swipe-trailing removes an episode (marks it played so it leaves the list).
struct ContinueListeningView: View {
    let episodes: [Episode]

    @Environment(AppStateStore.self) private var store
    @Environment(PlaybackState.self) private var playback

    var body: some View {
        List {
            ForEach(episodes) { ep in
                ContinueListeningRow(
                    episode: ep,
                    podcast: store.podcast(id: ep.podcastID),
                    onPlay: { playEpisode(ep) }
                )
                .swipeActions(edge: .trailing, allowsFullSwipe: true) {
                    Button(role: .destructive) {
                        Haptics.warning()
                        store.resetEpisodeProgress(ep.id)
                    } label: {
                        Label("Remove", systemImage: "xmark.circle")
                    }
                }
                .swipeActions(edge: .leading, allowsFullSwipe: true) {
                    EpisodeRowLeadingSwipeAction(episode: ep, playback: playback)
                }
                .listRowInsets(EdgeInsets(
                    top: AppTheme.Spacing.sm,
                    leading: AppTheme.Spacing.md,
                    bottom: AppTheme.Spacing.sm,
                    trailing: AppTheme.Spacing.md
                ))
                .listRowBackground(Color(.secondarySystemBackground))
            }
        }
        .listStyle(.insetGrouped)
        .navigationTitle("Continue Listening")
        .navigationBarTitleDisplayMode(.inline)
        .background(Color(.systemGroupedBackground).ignoresSafeArea())
        .overlay {
            if episodes.isEmpty {
                ContentUnavailableView(
                    "All Caught Up",
                    systemImage: "checkmark.circle",
                    description: Text("No in-progress episodes from the last 2 weeks.")
                )
            }
        }
    }

    private func playEpisode(_ episode: Episode) {
        Haptics.medium()
        playback.setEpisode(episode)
        playback.play()
    }
}
