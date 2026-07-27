import SwiftUI

// MARK: - SubscriptionContextMenu

/// Shared context menu for subscription rows and grid cells.
struct SubscriptionContextMenu: View {
    let podcast: Podcast
    /// `false` for podcasts the app knows about but the user does not
    /// follow. There is no subscription to leave in that case, so the
    /// destructive action reads "Delete" — it removes the podcast row and
    /// its episodes outright.
    var isFollowed: Bool = true
    let onRequestRemove: () -> Void

    @Environment(AppStateStore.self) private var store

    var body: some View {
        Button {
            Task { await SubscriptionService(store: store).refresh(podcast) }
        } label: {
            Label("Refresh", systemImage: "arrow.clockwise")
        }

        Button(role: .destructive) {
            onRequestRemove()
        } label: {
            if isFollowed {
                Label("Unsubscribe", systemImage: "minus.circle")
            } else {
                Label("Delete", systemImage: "trash")
            }
        }
    }
}
