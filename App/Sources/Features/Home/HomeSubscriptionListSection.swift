import SwiftUI

// MARK: - HomeSubscriptionListSection

/// The subscription surface beneath the featured rail. Renders as a vertical
/// list of followed podcasts, recency-sorted, honouring the active
/// LibraryFilter + category filter the parent owns.
///
/// The header carries a "See all" affordance that pushes
/// `AllPodcastsListView` — a separate screen that lists every known
/// podcast (followed AND unfollowed) with swipe-to-delete. Home keeps
/// showing only followed shows so the everyday surface stays focused on
/// what the user actually follows.
struct HomeSubscriptionListSection: View {
    let podcasts: [Podcast]
    let now: Date
    let onRequestUnsubscribe: (Podcast) -> Void
    let onSeeAllPodcasts: () -> Void

    @Environment(AppStateStore.self) private var store

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            rowList
        }
    }

    private var header: some View {
        HStack(spacing: AppTheme.Spacing.sm) {
            Text("Podcasts")
                .font(AppTheme.Typography.title3)
                .foregroundStyle(.primary)
            Spacer(minLength: 0)
            Button(action: {
                Haptics.selection()
                onSeeAllPodcasts()
            }) {
                Text("See All")
                    .font(AppTheme.Typography.subheadline)
                    .foregroundStyle(.tint)
            }
            .buttonStyle(.plain)
            .accessibilityLabel("See all podcasts")
        }
        .padding(.horizontal, AppTheme.Spacing.md)
        .padding(.bottom, AppTheme.Spacing.xs)
    }

    private var rowList: some View {
        LazyVStack(alignment: .leading, spacing: 0) {
            ForEach(podcasts) { sub in
                HomeSubscriptionRow(
                    podcast: sub,
                    mostRecentEpisode: store.mostRecentEpisode(forPodcast: sub.id),
                    now: now,
                    onRequestUnsubscribe: { onRequestUnsubscribe(sub) }
                )
                .padding(.horizontal, AppTheme.Spacing.md)
                .padding(.vertical, AppTheme.Spacing.sm)
                Divider()
                    .background(AppTheme.Tint.hairline)
                    .padding(.leading, AppTheme.Spacing.md + 53 + AppTheme.Spacing.md)
            }
        }
    }
}
