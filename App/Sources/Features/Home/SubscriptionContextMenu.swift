import SwiftUI

// MARK: - SubscriptionContextMenu

/// Shared context menu for subscription rows and grid cells.
struct SubscriptionContextMenu: View {
    let podcast: Podcast
    let onRequestUnsubscribe: () -> Void

    @Environment(AppStateStore.self) private var store

    var body: some View {
        Button {
            Task { await SubscriptionService(store: store).refresh(podcast) }
        } label: {
            Label("Refresh", systemImage: "arrow.clockwise")
        }

        Button(role: .destructive) {
            onRequestUnsubscribe()
        } label: {
            Label("Unsubscribe", systemImage: "minus.circle")
        }
    }
}
