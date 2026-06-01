import SwiftUI

/// Full-screen editor for the user's configured app relays (NMP v0.2.1
/// `configured_relays`). Reads the kernel projection directly — relay state is
/// kernel-owned, and every edit dispatches through `AppStateStore` and refreshes
/// via the next snapshot tick (no optimistic local mirror; see the reactivity
/// note on the `kernelAddRelay` helpers).
struct AppRelaysView: View {
    @Environment(AppStateStore.self) private var store

    @State private var showingAdd = false
    @State private var roleEditTarget: AppRelayRow?

    private var relays: [AppRelayRow] {
        store.kernel?.podcastSnapshot?.configuredRelays ?? []
    }

    var body: some View {
        Group {
            if relays.isEmpty {
                emptyState
            } else {
                relayList
            }
        }
        .navigationTitle("App Relays")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button {
                    showingAdd = true
                } label: {
                    Label("Add Relay", systemImage: "plus")
                }
            }
        }
        .sheet(isPresented: $showingAdd) {
            AddRelaySheet()
        }
        .sheet(item: $roleEditTarget) { relay in
            ChangeRelayRoleSheet(relay: relay)
        }
    }

    private var relayList: some View {
        List {
            Section {
                ForEach(relays) { relay in
                    Button {
                        roleEditTarget = relay
                    } label: {
                        AppRelayRowView(relay: relay)
                    }
                    .buttonStyle(.plain)
                    .swipeActions(edge: .trailing) {
                        Button(role: .destructive) {
                            store.kernelRemoveRelay(url: relay.url)
                        } label: {
                            Label("Remove", systemImage: "trash")
                        }
                    }
                }
            } footer: {
                Text("Tap a relay to change its role, or swipe to remove it. Relays carry NIP-65 read/write/indexer roles for Nostr discovery and publishing.")
            }
        }
        .settingsListStyle()
    }

    private var emptyState: some View {
        ContentUnavailableView {
            Label("No app relays configured", systemImage: "network.slash")
        } description: {
            Text("Add a relay to enable Nostr discovery and publishing.")
        } actions: {
            Button {
                showingAdd = true
            } label: {
                Label("Add Relay", systemImage: "plus")
            }
            .buttonStyle(.borderedProminent)
        }
    }
}

/// One relay row: host + full URL on the left, color-coded role pill trailing.
private struct AppRelayRowView: View {
    let relay: AppRelayRow

    var body: some View {
        HStack(spacing: 10) {
            VStack(alignment: .leading, spacing: 2) {
                Text(displayHost)
                    .font(.body)
                    .foregroundStyle(.primary)
                Text(relay.url)
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            Spacer(minLength: 8)
            AppRelayBadge(role: relay.role)
        }
        .padding(.vertical, 2)
        .contentShape(Rectangle())
    }

    private var displayHost: String {
        URL(string: relay.url)?.host ?? relay.url
    }
}

#Preview {
    NavigationStack {
        AppRelaysView()
            .environment(AppStateStore())
    }
}
