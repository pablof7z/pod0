import SwiftUI

struct AgentMemoriesView: View {
    @Environment(AppStateStore.self) private var store

    /// When non-nil (set by a Spotlight continuation), the view opens the edit
    /// sheet for this memory automatically on first appear.
    var spotlightTargetID: UUID? = nil

    @State private var searchText = ""
    @State private var showClearConfirm = false
    @State private var editingMemory: MemoryFact? = nil

    // MARK: - Derived data

    private var memoryFacts: [MemoryFact] {
        store.kernel?.podcastSnapshot?.memoryFacts ?? []
    }

    private var filteredMemories: [MemoryFact] {
        let all = memoryFacts.sorted { $0.createdAt > $1.createdAt }
        if searchText.isEmpty { return all }
        return all.filter {
            $0.value.localizedCaseInsensitiveContains(searchText)
                || $0.key.localizedCaseInsensitiveContains(searchText)
        }
    }

    /// Memories grouped by relative-date bucket, preserving reverse-chron order within each group.
    private var groupedMemories: [(bucket: RelativeDateBucket, items: [MemoryFact])] {
        RelativeDateBucket.grouped(filteredMemories, dateKey: { Date(timeIntervalSince1970: TimeInterval($0.createdAt)) })
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
            EditTextSheet(title: "Edit Memory", initialText: memory.value) { newContent in
                remember(key: memory.key, value: newContent, source: memory.source)
            }
        }
        .alert("Clear All Memories?", isPresented: $showClearConfirm) {
            Button("Clear All", role: .destructive) {
                forgetAll()
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
              let memory = memoryFacts.first(where: { $0.id == id.uuidString })
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
                    .symbolEffect(.pulse, isActive: memoryFacts.isEmpty)
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
                            copyText: memory.value,
                            onDelete: { forget(key: memory.key) }
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
        if !memoryFacts.isEmpty {
            ToolbarItem(placement: .destructiveAction) {
                Button("Clear All", role: .destructive) {
                    showClearConfirm = true
                }
            }
        }
    }

    private func remember(key: String, value: String, source: String) {
        store.kernel?.dispatch(namespace: "podcast.memory",
                               body: ["op": "remember", "key": key, "value": value, "source": source])
    }

    private func forget(key: String) {
        store.kernel?.dispatch(namespace: "podcast.memory",
                               body: ["op": "forget", "key": key])
    }

    private func forgetAll() {
        store.kernel?.dispatch(namespace: "podcast.memory",
                               body: ["op": "forget_all"])
    }

    // MARK: - MemoryRow

    private struct MemoryRow: View {
        let memory: MemoryFact
        var query: String = ""

        var body: some View {
            AgentContentRow(
                icon: "brain",
                iconColor: .purple,
                text: memory.value,
                date: Date(timeIntervalSince1970: TimeInterval(memory.createdAt)),
                badge: memory.key,
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
