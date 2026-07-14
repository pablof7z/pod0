import SwiftUI

/// List of every Nostr conversation the agent has participated in.
/// Reached from Settings > Agent > Conversations.
struct NostrConversationsView: View {

    @Environment(AppStateStore.self) private var store

    /// Optional NIP-10 conversation root id to auto-open on appear. Set
    /// when this view is reached via a deep-link so the user lands on the
    /// thread that triggered the routing, not the list.
    var initialRootID: String?

    @State private var pushedRootID: String?
    @State private var openedRootID: String?

    init(initialRootID: String? = nil) {
        self.initialRootID = initialRootID
    }

    var body: some View {
        List {
            if store.state.nostrConversations.isEmpty {
                ContentUnavailableView {
                    Label("No conversations yet", systemImage: "bubble.left.and.bubble.right")
                } description: {
                    Text("When your agent receives a Nostr mention and replies, the thread will appear here.")
                }
                .listRowBackground(Color.clear)
            } else {
                ForEach(sortedConversations) { conv in
                    Button {
                        Haptics.selection()
                        openedRootID = conv.rootEventID
                    } label: {
                        NostrConversationRow(
                            conv: conv,
                            profile: store.state.nostrProfileCache[conv.counterpartyPubkey]
                        )
                    }
                    .buttonStyle(.plain)
                }
            }
        }
        .settingsListStyle()
        .navigationTitle("Conversations")
        .navigationBarTitleDisplayMode(.inline)
        .navigationDestination(
            isPresented: Binding(
                get: { openedRootID != nil },
                set: { if !$0 { openedRootID = nil } }
            )
        ) {
            if let rootID = openedRootID,
               let conv = store.state.nostrConversations.first(where: { $0.rootEventID == rootID }) {
                NostrConversationDetailView(conversation: conv)
            } else {
                ContentUnavailableView(
                    "Conversation not found",
                    systemImage: "bubble.left.and.bubble.right",
                    description: Text("The thread may have been cleared from history.")
                )
            }
        }
        .onAppear {
            if let rootID = initialRootID,
               pushedRootID != rootID,
               store.state.nostrConversations.contains(where: { $0.rootEventID == rootID }) {
                pushedRootID = rootID
                openedRootID = rootID
            }
        }
    }

    private var sortedConversations: [NostrConversationRecord] {
        store.state.nostrConversations.sorted { $0.lastTouched > $1.lastTouched }
    }
}

// MARK: - Row

private struct NostrConversationRow: View {
    let conv: NostrConversationRecord
    let profile: NostrProfileMetadata?

    var body: some View {
        HStack(alignment: .top, spacing: AppTheme.Spacing.md) {
            NostrProfileAvatar(profile: profile)
                .frame(width: AppTheme.Layout.iconSm, height: AppTheme.Layout.iconSm)

            VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
                HStack(alignment: .firstTextBaseline) {
                    Text(primaryLabel)
                        .font(AppTheme.Typography.headline)
                        .lineLimit(1)
                        .truncationMode(.middle)
                    Spacer(minLength: AppTheme.Spacing.sm)
                    Text(relative(conv.lastTouched))
                        .font(AppTheme.Typography.caption2)
                        .foregroundStyle(.secondary)
                }
                if let secondary = secondaryLabel {
                    Text(secondary)
                        .font(AppTheme.Typography.mono)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
                if let last = conv.turns.last {
                    Text(last.content)
                        .font(AppTheme.Typography.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                }
                HStack(spacing: AppTheme.Spacing.sm) {
                    Label("\(incoming) in", systemImage: "arrow.down")
                    Label("\(outgoing) out", systemImage: "arrow.up")
                }
                .labelStyle(.titleAndIcon)
                .font(AppTheme.Typography.caption2)
                .foregroundStyle(.secondary)
            }
        }
        .padding(.vertical, AppTheme.Spacing.xs)
    }

    private var primaryLabel: String {
        if let label = profile?.bestLabel { return label }
        return NostrNpub.shortNpub(fromHex: conv.counterpartyPubkey)
    }

    private var secondaryLabel: String? {
        guard profile?.bestLabel != nil else { return nil }
        return NostrNpub.shortNpub(fromHex: conv.counterpartyPubkey)
    }

    private var incoming: Int { conv.turns.filter { $0.direction == .incoming }.count }
    private var outgoing: Int { conv.turns.filter { $0.direction == .outgoing }.count }

    private static let relativeFormatter: RelativeDateTimeFormatter = {
        let f = RelativeDateTimeFormatter()
        f.unitsStyle = .abbreviated
        return f
    }()

    private func relative(_ date: Date) -> String {
        Self.relativeFormatter.localizedString(for: date, relativeTo: Date())
    }
}

// MARK: - Avatar

struct NostrProfileAvatar: View {
    let profile: NostrProfileMetadata?

    var body: some View {
        if let url = profile?.pictureURL {
            AsyncImage(url: url) { phase in
                switch phase {
                case .success(let image):
                    image.resizable().scaledToFill()
                case .failure, .empty:
                    placeholder
                @unknown default:
                    placeholder
                }
            }
            .clipShape(Circle())
            .overlay(Circle().strokeBorder(AppTheme.Tint.hairline, lineWidth: 0.5))
        } else {
            placeholder
        }
    }

    private var placeholder: some View {
        ZStack {
            Circle().fill(AppTheme.Tint.placeholder)
            Image(systemName: "person.crop.circle.fill")
                .resizable()
                .foregroundStyle(.secondary)
        }
    }
}
