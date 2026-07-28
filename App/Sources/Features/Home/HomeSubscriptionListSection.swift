import SwiftUI

// MARK: - HomeSubscriptionListSection

/// The primary Home subscription surface. Renders as a vertical
/// list of followed podcasts, recency-sorted, honouring the active
/// LibraryFilter + category filter the parent owns.
///
/// Podcasts the app knows about but the user does NOT follow are appended
/// below the followed ones, under their own "Not Following" heading. They
/// are reachable nowhere else, so without this they would be stranded in
/// the store — see `sortedUnfollowedPodcastsByRecency`.
struct HomeSubscriptionListSection: View {
    let podcasts: [Podcast]
    let unfollowedPodcasts: [Podcast]
    let now: Date
    let onRequestUnsubscribe: (Podcast) -> Void
    let onRequestDelete: (Podcast) -> Void

    @Environment(AppStateStore.self) private var store

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if !podcasts.isEmpty {
                heading("Podcasts")
                rowList(podcasts, isFollowed: true)
            }
            if !unfollowedPodcasts.isEmpty {
                heading("Not Following")
                    .padding(.top, podcasts.isEmpty ? 0 : AppTheme.Spacing.lg)
                rowList(unfollowedPodcasts, isFollowed: false)
            }
        }
    }

    private func heading(_ title: String) -> some View {
        Text(title)
            .font(AppTheme.Typography.title3)
            .foregroundStyle(.primary)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, AppTheme.Spacing.md)
            .padding(.bottom, AppTheme.Spacing.xs)
    }

    private func rowList(_ rows: [Podcast], isFollowed: Bool) -> some View {
        LazyVStack(alignment: .leading, spacing: 0) {
            ForEach(rows) { sub in
                HomeSubscriptionRow(
                    podcast: sub,
                    mostRecentEpisode: store.mostRecentEpisode(forPodcast: sub.id),
                    now: now,
                    isFollowed: isFollowed,
                    onRequestRemove: {
                        if isFollowed {
                            onRequestUnsubscribe(sub)
                        } else {
                            onRequestDelete(sub)
                        }
                    }
                )
                .padding(.horizontal, AppTheme.Spacing.md)
                .padding(.vertical, AppTheme.Spacing.sm)
                Divider()
                    .background(AppTheme.Tint.hairline)
                    .padding(.leading, AppTheme.Spacing.md + AppTheme.Layout.iconLg + AppTheme.Spacing.md)
            }
        }
    }
}
