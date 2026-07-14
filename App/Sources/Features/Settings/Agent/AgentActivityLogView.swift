import SwiftUI

// MARK: - AgentActivityLogView

/// Full activity-log browser: all agent actions across every batch, with
/// search, category filter, date-bucket grouping, and per-row undo via
/// the existing `AgentActivitySheet`.
struct AgentActivityLogView: View {

    private enum ActivityFilterCategory: String, CaseIterable, Identifiable {
        case all = "All"
        case notes = "Notes"
        case memories = "Memories"

        var id: String { rawValue }

        var icon: String {
            switch self {
            case .all:       "line.3.horizontal.decrease.circle"
            case .notes:     "note.text"
            case .memories:  "brain"
            }
        }

        func matches(_ entry: AgentActivityEntry) -> Bool {
            switch self {
            case .all:
                return true
            case .notes:
                if case .noteCreated = entry.kind { return true }
                return false
            case .memories:
                if case .memoryRecorded = entry.kind { return true }
                return false
            }
        }
    }

    private enum Layout {
        static let chipHPadding: CGFloat = 10
        static let chipVPadding: CGFloat = 6
        static let chipIconSize: CGFloat = 11
    }

    @Environment(AppStateStore.self) private var store
    @State private var searchText = ""
    @State private var selectedCategory: ActivityFilterCategory = .all
    @State private var presentedBatch: UUID?

    // MARK: - Derived data

    private var allEntries: [AgentActivityEntry] {
        store.sortedAgentActivity
    }

    private var filteredEntries: [AgentActivityEntry] {
        allEntries.filter { entry in
            guard selectedCategory.matches(entry) else { return false }
            guard !searchText.isEmpty else { return true }
            return entry.summary.localizedCaseInsensitiveContains(searchText)
        }
    }

    private var groupedEntries: [(bucket: RelativeDateBucket, items: [AgentActivityEntry])] {
        RelativeDateBucket.grouped(filteredEntries, dateKey: \.timestamp)
    }

    private var categoryCounts: [ActivityFilterCategory: Int] {
        var counts: [ActivityFilterCategory: Int] = [:]
        for category in ActivityFilterCategory.allCases {
            counts[category] = category == .all
                ? allEntries.count
                : allEntries.filter { category.matches($0) }.count
        }
        return counts
    }

    // MARK: - Body

    var body: some View {
        List {
            filterChipsSection
            if filteredEntries.isEmpty {
                emptyState
            } else {
                entrySections
            }
        }
        .navigationTitle("Activity Log")
        .navigationBarTitleDisplayMode(.large)
        .searchable(text: $searchText, prompt: "Search activity")
        .sheet(item: $presentedBatch) { batchID in
            AgentActivitySheet(batchID: batchID)
        }
    }

    // MARK: - Subviews

    private var filterChipsSection: some View {
        Section {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: AppTheme.Spacing.xs) {
                    ForEach(ActivityFilterCategory.allCases) { category in
                        let count = categoryCounts[category] ?? 0
                        let isSelected = selectedCategory == category
                        Button {
                            selectedCategory = category
                            Haptics.selection()
                        } label: {
                            HStack(spacing: AppTheme.Spacing.xs) {
                                Image(systemName: category.icon)
                                    .font(.system(size: Layout.chipIconSize, weight: .medium))
                                Text(category.rawValue)
                                    .font(AppTheme.Typography.caption.weight(isSelected ? .semibold : .regular))
                                if count > 0 {
                                    Text("\(count)")
                                        .font(AppTheme.Typography.caption2)
                                        .foregroundStyle(isSelected ? Color.white.opacity(0.75) : Color.secondary)
                                        .monospacedDigit()
                                }
                            }
                            .foregroundStyle(isSelected ? Color.white : Color.primary)
                            .padding(.horizontal, Layout.chipHPadding)
                            .padding(.vertical, Layout.chipVPadding)
                            .background(isSelected ? Color.accentColor : Color.secondary.opacity(0.12), in: Capsule())
                        }
                        .buttonStyle(.plain)
                        .accessibilityAddTraits(isSelected ? .isSelected : [])
                        // `.isSelected` trait drives VoiceOver's "selected"
                        // announcement; the previous shape also appended
                        // ", selected" to the label, causing a double-narrate.
                        .accessibilityLabel("\(category.rawValue), \(count) item\(count == 1 ? "" : "s")")
                    }
                }
                .padding(.horizontal, AppTheme.Spacing.md)
                .padding(.vertical, AppTheme.Spacing.xs)
            }
            .listRowInsets(.init())
            .listRowBackground(Color.clear)
            .listRowSeparator(.hidden)
            .animation(AppTheme.Animation.springFast, value: selectedCategory)
        }
    }

    @ViewBuilder
    private var emptyState: some View {
        if allEntries.isEmpty {
            ContentUnavailableView {
                Label("No activity yet", systemImage: "clock.arrow.circlepath")
                    .symbolEffect(.pulse)
            } description: {
                Text("The agent's actions will appear here.")
            }
            .listRowBackground(Color.clear)
        } else if !searchText.isEmpty {
            ContentUnavailableView.search(text: searchText)
                .listRowBackground(Color.clear)
        } else {
            ContentUnavailableView {
                Label("No results", systemImage: "line.3.horizontal.decrease.circle")
            } description: {
                Text("Try a different filter.")
            }
            .listRowBackground(Color.clear)
        }
    }

    @ViewBuilder
    private var entrySections: some View {
        ForEach(groupedEntries, id: \.bucket) { group in
            Section {
                ForEach(group.items) { entry in
                    ActivityLogRow(entry: entry, query: searchText) {
                        presentedBatch = entry.batchID
                    }
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

    // MARK: - ActivityLogRow

    private struct ActivityLogRow: View {

    private enum Layout {
        static let iconColumnWidth: CGFloat = 22
        static let rowVerticalSpacing: CGFloat = 2
    }

    let entry: AgentActivityEntry
    var query: String = ""
    let onTap: () -> Void

    var body: some View {
        Button(action: onTap) {
            HStack(alignment: .firstTextBaseline, spacing: AppTheme.Spacing.md) {
                Image(systemName: entry.kind.icon)
                    .foregroundStyle(entry.undone
                        ? AnyShapeStyle(.tertiary)
                        : AnyShapeStyle(entry.kind.tint)
                    )
                    .font(AppTheme.Typography.callout)
                    .frame(width: Layout.iconColumnWidth)
                    .accessibilityHidden(true)

                VStack(alignment: .leading, spacing: Layout.rowVerticalSpacing) {
                    if query.isEmpty {
                        Text(entry.summary)
                            .font(AppTheme.Typography.body)
                            .foregroundStyle(entry.undone ? .secondary : .primary)
                            .strikethrough(entry.undone, color: .secondary)
                            .lineLimit(2)
                            .multilineTextAlignment(.leading)
                    } else {
                        HighlightedText(text: entry.summary, query: query)
                            .font(AppTheme.Typography.body)
                            .foregroundStyle(entry.undone ? .secondary : .primary)
                            .strikethrough(entry.undone, color: .secondary)
                            .lineLimit(2)
                            .multilineTextAlignment(.leading)
                    }

                    Text(RelativeTimestamp.extended(entry.timestamp))
                        .font(AppTheme.Typography.caption2)
                        .foregroundStyle(.tertiary)
                        .monospacedDigit()
                }

                Spacer(minLength: AppTheme.Spacing.sm)

                if entry.undone {
                    Image(systemName: "arrow.uturn.backward.circle.fill")
                        .foregroundStyle(.secondary)
                        .font(AppTheme.Typography.title3)
                        .accessibilityLabel("Undone")
                } else {
                    Image(systemName: "square.stack.3d.up")
                        .font(AppTheme.Typography.caption)
                        .foregroundStyle(.tertiary)
                        .accessibilityLabel("View batch")
                }
            }
            .padding(.vertical, AppTheme.Spacing.xs)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
    }
}

