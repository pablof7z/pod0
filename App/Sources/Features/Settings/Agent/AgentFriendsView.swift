import SwiftUI

struct AgentFriendsView: View {

    private enum FriendSortOrder: String, CaseIterable {
        case alphabetical    = "alphabetical"
        case mostRecent      = "mostRecent"

        var label: String {
            switch self {
            case .alphabetical: return "Alphabetical"
            case .mostRecent:   return "Most Recently Active"
            }
        }

        var icon: String {
            switch self {
            case .alphabetical: return "textformat.abc"
            case .mostRecent:   return "clock"
            }
        }
    }

    @Environment(AppStateStore.self) private var store
    @State private var showAddFriend = false
    @State private var pendingInvite: PendingFriendInvite? = nil
    @State private var searchText = ""
    @AppStorage("agentFriendsSortOrder") private var sortOrder: FriendSortOrder = .alphabetical

    // MARK: - Sorted + filtered friends

    /// Friends sorted by the active `sortOrder`, then optionally narrowed by `searchText`.
    private var filteredFriends: [Friend] {
        let sorted = sortedFriends
        let trimmed = searchText.trimmed
        guard !trimmed.isEmpty else { return sorted }
        return sorted.filter {
            $0.displayName.localizedCaseInsensitiveContains(trimmed) ||
            $0.identifier.localizedCaseInsensitiveContains(trimmed)
        }
    }

    private var sortedFriends: [Friend] {
        switch sortOrder {
        case .alphabetical:
            return store.sortedFriends
        case .mostRecent:
            return store.sortedFriends.sorted { lhs, rhs in
                let lDate = store.lastActivity(forFriend: lhs.id) ?? lhs.addedAt
                let rDate = store.lastActivity(forFriend: rhs.id) ?? rhs.addedAt
                return lDate > rDate
            }
        }
    }

    // MARK: - Sort menu

    /// Toolbar menu for choosing the friend list sort order.
    private var sortMenu: some View {
        let isNonDefault = sortOrder != .alphabetical
        let iconName = isNonDefault ? "arrow.up.arrow.down.circle.fill" : "arrow.up.arrow.down.circle"
        return Menu {
            ForEach(FriendSortOrder.allCases, id: \.self) { order in
                Button {
                    sortOrder = order
                    Haptics.selection()
                } label: {
                    Label(order.label, systemImage: order.icon)
                }
            }
        } label: {
            Image(systemName: iconName)
                .foregroundStyle(isNonDefault ? Color.accentColor : Color.secondary)
                .symbolEffect(.bounce, value: sortOrder)
        }
        .accessibilityLabel("Sort friends: \(sortOrder.label)")
    }

    var body: some View {
        List {
            if store.sortedFriends.isEmpty {
                ContentUnavailableView {
                    Label("No friends yet", systemImage: "person.2")
                } description: {
                    Text("Add a friend by their Nostr public key (npub or hex).")
                } actions: {
                    Button("Add Friend") { showAddFriend = true }
                        .buttonStyle(.glassProminent)
                }
                .listRowBackground(Color.clear)
            } else if filteredFriends.isEmpty {
                ContentUnavailableView.search(text: searchText)
                    .listRowBackground(Color.clear)
            } else {
                ForEach(filteredFriends) { friend in
                    NavigationLink {
                        FriendDetailView(friend: friend)
                    } label: {
                        FriendListRow(
                            friend: friend,
                            lastActivityDate: store.lastActivity(forFriend: friend.id),
                            query: searchText
                        )
                    }
                    .swipeActions(edge: .trailing, allowsFullSwipe: false) {
                        Button(role: .destructive) {
                            store.removeFriend(friend.id)
                            Haptics.delete()
                        } label: {
                            Label("Remove", systemImage: "trash")
                        }
                    }
                }
            }
        }
        .navigationTitle("Friends")
        .navigationBarTitleDisplayMode(.large)
        .searchable(text: $searchText, prompt: "Search friends")
        .toolbar {
            if !store.sortedFriends.isEmpty {
                ToolbarItem(placement: .topBarTrailing) {
                    sortMenu
                }
            }
            ToolbarItem(placement: .topBarTrailing) {
                Button {
                    showAddFriend = true
                } label: {
                    Label("Add", systemImage: "plus")
                }
            }
        }
        .sheet(isPresented: $showAddFriend) {
            AddFriendSheet()
        }
        .sheet(item: Binding(
            get: { pendingInvite },
            set: { pendingInvite = $0 }
        )) { invite in
            AddFriendSheet(prefillNpub: invite.npub, prefillName: invite.name)
        }
        .onChange(of: store.pendingFriendInvite) { _, invite in
            guard let invite else { return }
            store.pendingFriendInvite = nil   // consume immediately — fires exactly once
            pendingInvite = invite
        }
    }

    // MARK: - FriendListRow

    private struct FriendListRow: View {

    private enum Layout {
        static let avatarSize: CGFloat = 40
        static let rowSpacing: CGFloat = 12
        static let labelSpacing: CGFloat = 2
        static let trailingSpacerMin: CGFloat = 8
        static let rowVerticalPadding: CGFloat = 2
    }

    let friend: Friend
    /// Most recent activity date; nil means fall back to addedAt.
    let lastActivityDate: Date?
    var query: String = ""

    private var trailingLabel: String {
        if let date = lastActivityDate {
            return RelativeTimestamp.extended(date)
        }
        return "Added " + friend.addedAt.shortDate
    }

    var body: some View {
        HStack(alignment: .top, spacing: Layout.rowSpacing) {
            FriendAvatar(friend: friend, size: Layout.avatarSize)

            VStack(alignment: .leading, spacing: Layout.labelSpacing) {
                Group {
                    if query.isEmpty {
                        Text(friend.displayName)
                    } else {
                        HighlightedText(text: friend.displayName, query: query)
                    }
                }
                .font(AppTheme.Typography.headline)

                Text(friend.shortIdentifier)
                    .font(AppTheme.Typography.monoCaption)
                    .foregroundStyle(.secondary)

                if let about = friend.about, !about.isEmpty {
                    Text(about)
                        .font(AppTheme.Typography.caption)
                        .foregroundStyle(.tertiary)
                        .lineLimit(2)
                }
            }

            Spacer(minLength: Layout.trailingSpacerMin)

            Text(trailingLabel)
                .font(AppTheme.Typography.caption)
                .foregroundStyle(.tertiary)
                .lineLimit(1)
        }
        .padding(.vertical, Layout.rowVerticalPadding)
    }
    }
}
