import SwiftUI

/// History picker shown from the chat toolbar's clock button. Lists every
/// persisted conversation; tapping one switches the chat session to that
/// thread, swipe-to-delete removes a thread, and the toolbar "+" starts a
/// fresh conversation.
struct AgentChatHistoryView: View {

    let history: ChatHistoryStore
    /// Conversation currently shown in the chat sheet. Marked with a checkmark
    /// in the list so the user can orient themselves.
    let currentID: UUID
    /// Called with the conversation the user picked. Dismissal is the parent's
    /// responsibility — it owns the sheet binding.
    let onSelect: (ChatConversation) -> Void
    /// Called when the user taps "New chat". Same dismissal contract as `onSelect`.
    let onNew: () -> Void

    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            Group {
                if history.conversations.isEmpty {
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
            ForEach(history.conversations) { convo in
                row(for: convo)
                    .contentShape(.rect)
                    .onTapGesture {
                        Haptics.selection()
                        onSelect(convo)
                        dismiss()
                    }
                    .swipeActions(edge: .trailing, allowsFullSwipe: true) {
                        Button(role: .destructive) {
                            Haptics.warning()
                            history.delete(convo.id)
                        } label: {
                            Label("Delete", systemImage: "trash")
                        }
                    }
            }
        }
    }

    private func row(for convo: ChatConversation) -> some View {
        HStack(alignment: .top, spacing: AppTheme.Spacing.md) {
            VStack(alignment: .leading, spacing: 2) {
                Text(rowTitle(for: convo))
                    .font(AppTheme.Typography.callout)
                    .foregroundStyle(.primary)
                    .lineLimit(1)
                Text(relativeTimestamp(convo.updatedAt))
                    .font(AppTheme.Typography.caption2)
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 0)
            if convo.id == currentID {
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
