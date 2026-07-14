import SwiftUI

/// Renders every Nostr conversation the user shares with `friendPubkey` as a
/// chat-like transcript, inlined into `FriendDetailView`'s list. Conversations
/// are grouped by NIP-10 root and ordered most-recently-touched first; turns
/// inside each conversation remain in chronological order so the eye can read
/// each thread top-to-bottom.
///
/// The bubble styling mirrors `NostrConversationDetailView.NostrTurnBubble`
/// (incoming on the left in `secondarySystemBackground`, outgoing on the right
/// in `AppTheme.Tint.agentSurface`) so a friend's transcript feels like the
/// same surface the Settings > Conversations screen renders — just scoped to
/// one counterparty.
struct FriendConversationTranscriptView: View {

    @Environment(AppStateStore.self) private var store

    /// Hex pubkey of the friend whose transcript should be shown.
    let friendPubkey: String

    var body: some View {
        let conversations = matchingConversations
        Section {
            if conversations.isEmpty {
                emptyState
            } else {
                ForEach(conversations) { conv in
                    conversationBlock(conv)
                }
            }
        } header: {
            Text("Messages")
        }
    }

    // MARK: - Data

    /// Conversations whose counterparty matches this friend's hex pubkey,
    /// ordered by most-recent activity first. `lastTouched` is the canonical
    /// recency signal — same field `NostrConversationsView` sorts on — so the
    /// ordering here matches what the user sees in Settings > Conversations.
    private var matchingConversations: [NostrConversationRecord] {
        store.state.nostrConversations
            .filter { $0.counterpartyPubkey == friendPubkey }
            .sorted { $0.lastTouched > $1.lastTouched }
    }

    // MARK: - Subviews

    private var emptyState: some View {
        Text("No messages with this friend yet.")
            .font(AppTheme.Typography.callout)
            .foregroundStyle(.secondary)
            .frame(maxWidth: .infinity, alignment: .leading)
    }

    @ViewBuilder
    private func conversationBlock(_ conv: NostrConversationRecord) -> some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
            if matchingConversations.count > 1 {
                conversationHeader(conv)
            }
            ForEach(conv.turns, id: \.eventID) { turn in
                FriendTranscriptBubble(turn: turn)
            }
        }
        .padding(.vertical, AppTheme.Spacing.xs)
        .listRowSeparator(.hidden)
    }

    private func conversationHeader(_ conv: NostrConversationRecord) -> some View {
        // Sub-header per thread when the friend has more than one NIP-10
        // root with us — keeps the boundaries visible without introducing
        // a new visual idiom; reuses caption typography from elsewhere in
        // settings rows.
        Text(conv.lastTouched, style: .date)
            .font(AppTheme.Typography.caption2)
            .foregroundStyle(.tertiary)
            .frame(maxWidth: .infinity, alignment: .center)
            .padding(.top, AppTheme.Spacing.xs)
    }
}

// MARK: - Bubble

/// Single chat bubble. Mirrors `NostrTurnBubble` in
/// `NostrConversationDetailView` — duplicated rather than extracted because
/// the original is `private` and lifting it into a shared component is out of
/// scope for this UI wiring. If a third caller needs the same bubble, promote
/// then.
private struct FriendTranscriptBubble: View {
    let turn: NostrConversationTurn

    var body: some View {
        HStack {
            if turn.direction == .outgoing {
                Spacer(minLength: AppTheme.Layout.bubbleSpacer)
            }
            VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
                Text(turn.content)
                    .font(AppTheme.Typography.body)
                    .foregroundStyle(turn.direction == .outgoing ? Color.white : Color.primary)
                Text(timestamp)
                    .font(AppTheme.Typography.caption2)
                    .foregroundStyle(turn.direction == .outgoing
                        ? Color.white.opacity(0.8)
                        : Color.secondary)
            }
            .padding(.horizontal, AppTheme.Spacing.md)
            .padding(.vertical, AppTheme.Spacing.sm)
            .background(
                RoundedRectangle(cornerRadius: AppTheme.Corner.bubble, style: .continuous)
                    .fill(turn.direction == .outgoing
                        ? AppTheme.Tint.agentSurface
                        : Color(.secondarySystemBackground))
            )
            if turn.direction == .incoming {
                Spacer(minLength: AppTheme.Layout.bubbleSpacer)
            }
        }
    }

    private var timestamp: String {
        turn.createdAt.formatted(date: .abbreviated, time: .shortened)
    }
}
