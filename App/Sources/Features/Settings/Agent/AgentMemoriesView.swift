import SwiftUI

struct AgentMemoriesView: View {
    @Environment(AppStateStore.self) private var store

    /// When non-nil (set by a Spotlight continuation), the view opens the edit
    /// sheet for this memory automatically on first appear.
    var spotlightTargetID: UUID? = nil

    @State private var searchText = ""
    @State private var showClearConfirm = false
    @State private var editingMemory: AgentMemory? = nil

    // MARK: - Derived data

    private var filteredMemories: [AgentMemory] {
        let all = store.activeMemories.sorted { $0.createdAt > $1.createdAt }
        if searchText.isEmpty { return all }
        return all.filter { $0.content.localizedCaseInsensitiveContains(searchText) }
    }

    /// Memories grouped by relative-date bucket, preserving reverse-chron order within each group.
    private var groupedMemories: [(bucket: RelativeDateBucket, items: [AgentMemory])] {
        RelativeDateBucket.grouped(filteredMemories, dateKey: \.createdAt)
    }

    // MARK: - Body

    var body: some View {
        List {
            if filteredMemories.isEmpty {
                emptyState
            } else {
                memorySections
            }
        }
        .navigationTitle("Memories")
        .navigationBarTitleDisplayMode(.large)
        .searchable(text: $searchText, prompt: "Search memories")
        .toolbar { toolbarContent }
        .sheet(item: $editingMemory) { memory in
            EditTextSheet(title: "Edit Memory", initialText: memory.content) { newContent in
                store.updateAgentMemory(memory.id, content: newContent)
            }
        }
        .alert("Clear All Memories?", isPresented: $showClearConfirm) {
            Button("Clear All", role: .destructive) {
                store.clearAllAgentMemories()
                Haptics.bulkAction()
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("The agent will lose everything it has learned about you. This cannot be undone.")
        }
        .onAppear { openSpotlightTargetIfNeeded() }
    }

    // MARK: - Spotlight continuation

    /// Opens the edit sheet for `spotlightTargetID` when the view appears
    /// as part of a Spotlight continuation. No-ops if the ID is nil or the
    /// memory has since been deleted.
    private func openSpotlightTargetIfNeeded() {
        guard let id = spotlightTargetID,
              let memory = store.activeMemories.first(where: { $0.id == id })
        else { return }
        Haptics.selection()
        Task { @MainActor in
            editingMemory = memory
        }
    }

    // MARK: - Subviews

    @ViewBuilder
    private var emptyState: some View {
        if searchText.isEmpty {
            ContentUnavailableView {
                Label("No memories yet", systemImage: "brain")
                    .symbolEffect(.pulse, isActive: store.activeMemories.isEmpty)
            } description: {
                Text("The agent will remember things about you as you interact.")
            }
            .listRowBackground(Color.clear)
        } else {
            ContentUnavailableView.search(text: searchText)
                .listRowBackground(Color.clear)
        }
    }

    @ViewBuilder
    private var memorySections: some View {
        ForEach(groupedMemories, id: \.bucket) { group in
            Section {
                ForEach(group.items) { memory in
                    MemoryRow(memory: memory, query: searchText)
                        .agentContentRowActions(
                            onEdit: { editingMemory = memory },
                            copyText: memory.content,
                            onDelete: { store.deleteAgentMemory(memory.id) }
                        )
                }
            } header: {
                HStack {
                    Text(group.bucket.rawValue)
                    Spacer()
                    Text("\(group.items.count)")
                        .monospacedDigit()
                }
            }
        }
    }

    @ToolbarContentBuilder
    private var toolbarContent: some ToolbarContent {
        if !store.activeMemories.isEmpty {
            ToolbarItem(placement: .destructiveAction) {
                Button("Clear All", role: .destructive) {
                    showClearConfirm = true
                }
            }
        }
    }

    // MARK: - MemoryRow

    private struct MemoryRow: View {
        let memory: AgentMemory
        var query: String = ""

        var body: some View {
            AgentContentRow(
                icon: "brain",
                iconColor: .purple,
                text: memory.content,
                date: memory.createdAt,
                query: query
            )
        }
    }
}

// MARK: - AgentContentRow (shared)

/// Reusable content row used by both `AgentMemoriesView` and `AgentNotesView`.
/// Shows an icon badge, multi-line body text, and a relative timestamp.
struct AgentContentRow: View {

    // MARK: - Layout constants

    private enum Layout {
        static let iconTopPadding: CGFloat = 2
        static let timestampLeadingOffset: CGFloat = 18
    }

    let icon: String
    let iconColor: Color
    let text: String
    let date: Date
    var badge: String? = nil
    /// When non-empty, occurrences of this query term are bolded in the body text.
    var query: String = ""

    var body: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
            HStack(alignment: .top) {
                Image(systemName: icon)
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(iconColor)
                    .padding(.top, Layout.iconTopPadding)
                    .accessibilityHidden(true)

                if query.isEmpty {
                    Text(text)
                        .font(AppTheme.Typography.callout)
                        .lineLimit(5)
                } else {
                    HighlightedText(text: text, query: query)
                        .font(AppTheme.Typography.callout)
                        .lineLimit(5)
                }
            }

            HStack(spacing: AppTheme.Spacing.xs) {
                if let badge {
                    Text(badge)
                        .font(AppTheme.Typography.caption2)
                        .foregroundStyle(iconColor)
                        .padding(.horizontal, AppTheme.Spacing.xs)
                        .padding(.vertical, 1)
                        .background(iconColor.opacity(0.10), in: Capsule())
                }
                Text(RelativeTimestamp.extended(date))
                    .font(AppTheme.Typography.mono)
                    .foregroundStyle(.tertiary)
            }
            .padding(.leading, Layout.timestampLeadingOffset)
        }
        .padding(.vertical, AppTheme.Spacing.xs)
    }
}
