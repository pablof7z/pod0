import SwiftUI

// MARK: - ShowDetailEpisodeList

/// Episode list inside `ShowDetailView`. Renders a `ForEach` of NMP-native
/// episode rows driven by `EpisodeSummary` from the kernel snapshot.
///
/// Play and Download dispatch directly to the `podcast.player` /
/// `podcast` namespaces — no `PlaybackState`, no `AppStateStore`. Download
/// progress UI lands in a follow-up PR once the `DownloadReport` back-channel
/// is wired; until then the snapshot's `downloadPath` flips from `nil` to a
/// path on `Completed`.
struct ShowDetailEpisodeList: View {
    let episodes: [EpisodeSummary]
    let podcast: PodcastSummary

    @Environment(KernelModel.self) private var model

    var body: some View {
        ForEach(episodes) { ep in
            KernelEpisodeRow(
                episode: ep,
                fallbackArtworkUrl: podcast.artworkUrl,
                onPlay: {
                    Haptics.medium()
                    model.dispatch(
                        namespace: "podcast.player",
                        body: ["op": "play", "episode_id": ep.id]
                    )
                    NotificationCenter.default.post(name: .openPlayerRequested, object: nil)
                },
                onDownload: ep.downloadPath == nil ? {
                    Haptics.light()
                    model.dispatch(
                        namespace: "podcast",
                        body: ["op": "download", "episode_id": ep.id]
                    )
                } : nil
            )
            .listRowSeparator(.hidden)
            .listRowInsets(EdgeInsets(
                top: AppTheme.Spacing.xs,
                leading: AppTheme.Spacing.lg,
                bottom: AppTheme.Spacing.xs,
                trailing: AppTheme.Spacing.lg
            ))
            .listRowBackground(Color(.systemBackground))
            .swipeActions(edge: .trailing, allowsFullSwipe: true) {
                if !ep.played {
                    Button {
                        markPlayed(ep)
                    } label: {
                        Label("Played", systemImage: "checkmark.circle.fill")
                    }
                    .tint(.green)
                }
            }
            .swipeActions(edge: .leading, allowsFullSwipe: true) {
                Button {
                    toggleStar(ep)
                } label: {
                    Label(ep.starred ? "Unbookmark" : "Bookmark",
                          systemImage: ep.starred ? "bookmark.slash" : "bookmark")
                }
                .tint(ep.starred ? .gray : .orange)
                Button {
                    enqueue(ep)
                } label: {
                    Label("Up Next", systemImage: "text.line.first.and.arrowtriangle.forward")
                }
                .tint(.accentColor)
            }
            .contextMenu {
                Button {
                    Haptics.light()
                    model.dispatch(
                        namespace: "podcast.queue",
                        body: ["op": "add_next", "episode_id": ep.id]
                    )
                } label: {
                    Label("Play Next", systemImage: "text.insert")
                }
                Button {
                    enqueue(ep)
                } label: {
                    Label("Add to Up Next", systemImage: "text.line.first.and.arrowtriangle.forward")
                }
                Button {
                    Haptics.light()
                    model.dispatch(
                        namespace: "podcast.queue",
                        body: ["op": "add_last", "episode_id": ep.id]
                    )
                } label: {
                    Label("Add to Queue", systemImage: "text.append")
                }
                Divider()
                Button {
                    toggleStar(ep)
                } label: {
                    Label(ep.starred ? "Remove Bookmark" : "Bookmark",
                          systemImage: ep.starred ? "bookmark.slash" : "bookmark")
                }
                if !ep.played {
                    Button {
                        markPlayed(ep)
                    } label: {
                        Label("Mark as Played", systemImage: "checkmark.circle")
                    }
                }
            }
        }
    }

    /// Dispatch `podcast.player.enqueue` — kernel dedups by id and
    /// surfaces the updated queue via `PodcastUpdate.queue` on the
    /// next snapshot tick (D7).
    private func enqueue(_ ep: EpisodeSummary) {
        Haptics.selection()
        model.dispatch(
            namespace: "podcast.player",
            body: ["op": "enqueue", "episode_id": ep.id]
        )
    }

    private func markPlayed(_ ep: EpisodeSummary) {
        Haptics.light()
        model.dispatch(
            namespace: "podcast.inbox",
            body: ["op": "mark_listened", "episode_id": ep.id]
        )
    }

    private func toggleStar(_ ep: EpisodeSummary) {
        Haptics.selection()
        model.dispatch(
            namespace: "podcast",
            body: ["op": "star_episode", "episode_id": ep.id]
        )
    }
}
