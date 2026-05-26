import SwiftUI

// MARK: - InboxView
//
// AI-triaged "what to listen to next" view (feature #31). Reads
// `model.podcastSnapshot?.inbox` directly — the kernel side
// (`crate::inbox_handler::build_inbox`) owns the policy: which
// episodes qualify, in what order, and what the priority caption is.
// The view is a thin renderer.
//
// Swipe-to-dismiss dispatches `podcast.inbox.dismiss`; the row's
// "Listened" swipe dispatches `podcast.inbox.mark_listened`. Pull-to-
// refresh dispatches `podcast.inbox.triage` (which itself just bumps
// the kernel rev so the next snapshot tick rebuilds the projection).

struct InboxView: View {
    @Environment(KernelModel.self) private var model

    var body: some View {
        NavigationStack {
            Group {
                if inboxItems.isEmpty {
                    emptyState
                } else {
                    inboxList
                }
            }
            .navigationTitle("Inbox")
            .navigationDestination(for: EpisodeRoute.self) { route in
                EpisodeDetailView(episode: route.episode, podcast: route.podcast)
            }
            .refreshable {
                model.dispatch(namespace: "podcast.inbox", body: ["op": "triage"])
            }
        }
    }

    // MARK: - Data

    private var inboxItems: [InboxItem] {
        model.podcastSnapshot?.inbox ?? []
    }

    private var emptyState: some View {
        ContentUnavailableView(
            "Inbox Clear",
            systemImage: "tray",
            description: Text("New episodes from your subscriptions will appear here.")
        )
    }

    // MARK: - List

    private var inboxList: some View {
        List {
            ForEach(inboxItems) { item in
                row(for: item)
                    .listRowSeparator(.hidden)
                    .listRowInsets(EdgeInsets(
                        top: AppTheme.Spacing.xs,
                        leading: AppTheme.Spacing.lg,
                        bottom: AppTheme.Spacing.xs,
                        trailing: AppTheme.Spacing.lg
                    ))
                    .listRowBackground(Color(.systemBackground))
                    .swipeActions(edge: .trailing, allowsFullSwipe: true) {
                        Button(role: .destructive) {
                            dismiss(item)
                        } label: {
                            Label("Dismiss", systemImage: "tray.and.arrow.down.fill")
                        }
                    }
                    .swipeActions(edge: .leading, allowsFullSwipe: false) {
                        Button {
                            markListened(item)
                        } label: {
                            Label("Listened", systemImage: "checkmark.circle.fill")
                        }
                        .tint(.green)
                        Button {
                            star(item)
                        } label: {
                            Label("Bookmark", systemImage: "bookmark")
                        }
                        .tint(.orange)
                    }
            }
        }
        .listStyle(.plain)
    }

    @ViewBuilder
    private func row(for item: InboxItem) -> some View {
        // Reconstruct the (episode, podcast) tuple `EpisodeRoute` expects
        // from the library snapshot. This keeps tap-to-detail using the
        // same navigation contract as `AllEpisodesView`. When the item's
        // podcast or episode is no longer in the library (e.g. an
        // intervening unsubscribe), fall back to a non-navigable row so
        // the inbox stays usable.
        if let route = makeRoute(for: item) {
            NavigationLink(value: route) {
                InboxEpisodeRow(item: item)
            }
            .buttonStyle(.plain)
        } else {
            InboxEpisodeRow(item: item)
        }
    }

    // MARK: - Actions

    private func dismiss(_ item: InboxItem) {
        Haptics.light()
        model.dispatch(
            namespace: "podcast.inbox",
            body: ["op": "dismiss", "episode_id": item.episodeId]
        )
    }

    private func markListened(_ item: InboxItem) {
        Haptics.medium()
        model.dispatch(
            namespace: "podcast.inbox",
            body: ["op": "mark_listened", "episode_id": item.episodeId]
        )
    }

    private func star(_ item: InboxItem) {
        Haptics.selection()
        model.dispatch(
            namespace: "podcast",
            body: ["op": "star_episode", "episode_id": item.episodeId]
        )
    }

    private func makeRoute(for item: InboxItem) -> EpisodeRoute? {
        guard let podcast = model.library.first(where: { $0.id == item.podcastId }),
              let episode = podcast.episodes.first(where: { $0.id == item.episodeId })
        else { return nil }
        return EpisodeRoute(episode: episode, podcast: podcast)
    }
}
