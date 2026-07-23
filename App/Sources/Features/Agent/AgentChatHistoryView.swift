import Pod0Core
import SwiftUI

/// History picker for Rust-owned durable conversations.
struct AgentChatHistoryView: View {
    let conversations: [AgentConversationSummaryProjection]
    let hasMore: Bool
    let currentID: ConversationId?
    let onSelect: (ConversationId) -> Void
    let onNew: () -> Void

    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            Group {
                if conversations.isEmpty {
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
            Section("Conversations") {
                ForEach(conversations, id: \.conversationId) { summary in
                    row(summary)
                        .contentShape(.rect)
                        .onTapGesture {
                            Haptics.selection()
                            onSelect(summary.conversationId)
                            dismiss()
                        }
                }
                if hasMore {
                    Text("Showing the 500 most recent conversations. Older history remains stored.")
                        .font(AppTheme.Typography.caption)
                        .foregroundStyle(.secondary)
                }
            }
        }
    }

    private func row(_ summary: AgentConversationSummaryProjection) -> some View {
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
            if summary.conversationId == currentID {
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

    private static let relativeFormatter: RelativeDateTimeFormatter = {
        let formatter = RelativeDateTimeFormatter()
        formatter.unitsStyle = .abbreviated
        return formatter
    }()

    private func relativeTimestamp(_ date: Date) -> String {
        Self.relativeFormatter.localizedString(for: date, relativeTo: Date())
    }
}
