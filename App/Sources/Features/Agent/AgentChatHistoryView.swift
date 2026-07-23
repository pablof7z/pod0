import SwiftUI
import Pod0Core

/// History picker for Rust-owned conversations plus the read-only Swift
/// archive retained for migration evidence.
struct AgentChatHistoryView: View {

    private let history: ChatHistoryStore
    let sharedConversations: [AgentConversationSummaryProjection]
    let sharedHasMore: Bool
    let currentSharedID: ConversationId?
    let currentLegacyID: UUID?
    let onSelectShared: (ConversationId) -> Void
    let onSelectLegacy: (ChatConversation) -> Void
    let onNew: () -> Void

    init(
        history: ChatHistoryStore = .shared,
        sharedConversations: [AgentConversationSummaryProjection],
        sharedHasMore: Bool,
        currentSharedID: ConversationId?,
        currentLegacyID: UUID?,
        onSelectShared: @escaping (ConversationId) -> Void,
        onSelectLegacy: @escaping (ChatConversation) -> Void,
        onNew: @escaping () -> Void
    ) {
        self.history = history
        self.sharedConversations = sharedConversations
        self.sharedHasMore = sharedHasMore
        self.currentSharedID = currentSharedID
        self.currentLegacyID = currentLegacyID
        self.onSelectShared = onSelectShared
        self.onSelectLegacy = onSelectLegacy
        self.onNew = onNew
    }

    static func archivedConversation(id: UUID) -> ChatConversation? {
        ChatHistoryStore.shared.conversation(id: id)
    }

    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            Group {
                if sharedConversations.isEmpty && history.conversations.isEmpty {
                    emptyState
                } else {
                    list
                }
            }
            .navigationTitle("History")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Done") { dismiss() }
                }
                ToolbarItem(placement: .primaryAction) {
                    Button {
                        Haptics.selection()
                        onNew()
                        dismiss()
                    } label: {
                        Image(systemName: "square.and.pencil")
                    }
                    .accessibilityLabel("New conversation")
                }
            }
        }
        .presentationDetents([.medium, .large])
        .presentationDragIndicator(.visible)
    }

    private var list: some View {
        List {
            if !sharedConversations.isEmpty {
                Section("Conversations") {
                    ForEach(sharedConversations, id: \.conversationId) { summary in
                        sharedRow(summary)
                            .contentShape(.rect)
                            .onTapGesture {
                                Haptics.selection()
                                onSelectShared(summary.conversationId)
                                dismiss()
                            }
                    }
                    if sharedHasMore {
                        Text("Showing the 500 most recent conversations. Older history remains stored.")
                            .font(AppTheme.Typography.caption)
                            .foregroundStyle(.secondary)
                    }
                }
            }
            if !history.conversations.isEmpty {
                Section("Archived from earlier versions") {
                    ForEach(history.conversations) { conversation in
                        legacyRow(conversation)
                            .contentShape(.rect)
                            .onTapGesture {
                                Haptics.selection()
                                onSelectLegacy(conversation)
                                dismiss()
                            }
                            .swipeActions(edge: .trailing, allowsFullSwipe: true) {
                                Button(role: .destructive) {
                                    Haptics.warning()
                                    history.delete(conversation.id)
                                } label: {
                                    Label("Delete", systemImage: "trash")
                                }
                        }
                    }
                }
            }
        }
    }

    private func sharedRow(_ summary: AgentConversationSummaryProjection) -> some View {
        HStack(alignment: .top, spacing: AppTheme.Spacing.md) {
            VStack(alignment: .leading, spacing: 2) {
                Text(summary.title.isBlank ? "New conversation" : summary.title)
                    .font(AppTheme.Typography.callout)
                    .foregroundStyle(.primary)
                    .lineLimit(1)
                if !summary.preview.isBlank && summary.preview != summary.title {
                    Text(summary.preview)
                        .font(AppTheme.Typography.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                Text(relativeTimestamp(summary.updatedAt.date))
                    .font(AppTheme.Typography.caption2)
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 0)
            if summary.conversationId == currentSharedID {
                Image(systemName: "checkmark")
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(AppTheme.Tint.agentSurface)
                    .accessibilityLabel("Current conversation")
            }
        }
        .padding(.vertical, 2)
    }

    private func legacyRow(_ conversation: ChatConversation) -> some View {
        HStack(alignment: .top, spacing: AppTheme.Spacing.md) {
            VStack(alignment: .leading, spacing: 2) {
                Text(rowTitle(for: conversation))
                    .font(AppTheme.Typography.callout)
                    .foregroundStyle(.primary)
                    .lineLimit(1)
                Text(relativeTimestamp(conversation.updatedAt))
                    .font(AppTheme.Typography.caption2)
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 0)
            if conversation.id == currentLegacyID {
                Image(systemName: "checkmark")
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(AppTheme.Tint.agentSurface)
                    .accessibilityLabel("Current conversation")
            }
        }
        .padding(.vertical, 2)
    }

    private var emptyState: some View {
        VStack(spacing: AppTheme.Spacing.sm) {
            Image(systemName: "bubble.left.and.bubble.right")
                .font(.system(size: 36, weight: .regular))
                .foregroundStyle(.secondary)
            Text("No past conversations")
                .font(AppTheme.Typography.callout)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private func rowTitle(for convo: ChatConversation) -> String {
        let title = convo.title.trimmingCharacters(in: .whitespacesAndNewlines)
        if !title.isEmpty { return title }
        let snippet = convo.firstUserSnippet.trimmingCharacters(in: .whitespacesAndNewlines)
        if !snippet.isEmpty {
            return String(snippet.prefix(60))
        }
        return "New conversation"
    }

    private static let relativeFormatter: RelativeDateTimeFormatter = {
        let f = RelativeDateTimeFormatter()
        f.unitsStyle = .abbreviated
        return f
    }()

    private func relativeTimestamp(_ date: Date) -> String {
        Self.relativeFormatter.localizedString(for: date, relativeTo: Date())
    }
}
