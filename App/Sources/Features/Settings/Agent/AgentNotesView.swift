import SwiftUI

// MARK: - AgentNotesView

struct AgentNotesView: View {

    private enum Layout {
        static let chipHPadding: CGFloat = 10
        static let chipVPadding: CGFloat = 6
        static let chipIconSize: CGFloat = 11
    }

    @Environment(AppStateStore.self) private var store

    /// When non-nil (set by a Spotlight continuation), the view opens the edit
    /// sheet for this note automatically on first appear.
    var spotlightTargetID: UUID? = nil

    @State private var searchText = ""
    @State private var showClearConfirm = false
    @State private var editingNote: Note? = nil
    @State private var showNewNote = false
    @State private var selectedKind: NoteKind?

    // MARK: - Derived

    private var activeNotes: [Note] {
        store.activeNotes
            .sorted { $0.createdAt > $1.createdAt }
    }

    private var availableKinds: [NoteKind] {
        let kinds = Set(activeNotes.map(\.kind))
        return [.free, .reflection, .systemEvent].filter { kinds.contains($0) }
    }

    private var filteredNotes: [Note] {
        var notes = activeNotes
        if let kind = selectedKind {
            notes = notes.filter { $0.kind == kind }
        }
        guard !searchText.isEmpty else { return notes }
        return notes.filter { $0.text.localizedCaseInsensitiveContains(searchText) }
    }

    private var groupedNotes: [(bucket: RelativeDateBucket, items: [Note])] {
        RelativeDateBucket.grouped(filteredNotes, dateKey: \.createdAt)
    }

    private var kindCounts: [NoteKind: Int] {
        var counts: [NoteKind: Int] = [:]
        for note in activeNotes {
            counts[note.kind, default: 0] += 1
        }
        return counts
    }

    // MARK: - Body

    var body: some View {
        List {
            if availableKinds.count > 1 {
                kindChipsSection
            }
            if filteredNotes.isEmpty {
                emptyState
            } else {
                noteSections
            }
        }
        .navigationTitle("Notes")
        .navigationBarTitleDisplayMode(.large)
        .searchable(text: $searchText, prompt: "Search notes")
        .toolbar { toolbarContent }
        .sheet(item: $editingNote) { note in
            EditTextSheet(title: "Edit Note", initialText: note.text) { newText in
                var updated = note
                updated.text = newText
                store.updateNote(updated)
            }
        }
        .sheet(isPresented: $showNewNote) {
            EditTextSheet(title: "New Note", initialText: "") { text in
                store.addNote(text: text, kind: .free)
                Haptics.success()
            }
        }
        .alert("Clear All Notes?", isPresented: $showClearConfirm) {
            Button("Clear All", role: .destructive) {
                store.clearAllNotes()
                Haptics.bulkAction()
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("All notes will be permanently deleted. This cannot be undone.")
        }
        .onAppear { openSpotlightTargetIfNeeded() }
    }

    // MARK: - Spotlight continuation

    /// Opens the edit sheet for `spotlightTargetID` when the view appears
    /// as part of a Spotlight continuation. No-ops if the ID is nil or the
    /// note has since been deleted.
    ///
    /// The assignment is deferred one run-loop tick (via `Task`) because the
    /// view is itself being presented inside a sheet when Spotlight routes here.
    /// Setting `editingNote` synchronously inside `.onAppear` often no-ops in
    /// SwiftUI when a parent presentation transaction is still in flight.
    private func openSpotlightTargetIfNeeded() {
        guard let id = spotlightTargetID,
              let note = activeNotes.first(where: { $0.id == id })
        else { return }
        Haptics.selection()
        Task { @MainActor in
            editingNote = note
        }
    }

    // MARK: - Subviews

    private var kindChipsSection: some View {
        Section {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: AppTheme.Spacing.xs) {
                    kindChip(kind: nil, label: "All", icon: "tray.2", count: activeNotes.count)
                    ForEach(availableKinds, id: \.self) { kind in
                        let (label, icon) = Self.chipInfo(for: kind)
                        kindChip(kind: kind, label: label, icon: icon, count: kindCounts[kind] ?? 0)
                    }
                }
                .padding(.horizontal, AppTheme.Spacing.md)
                .padding(.vertical, AppTheme.Spacing.xs)
            }
            .listRowInsets(.init())
            .listRowBackground(Color.clear)
            .listRowSeparator(.hidden)
            .animation(AppTheme.Animation.springFast, value: selectedKind)
        }
    }

    private func kindChip(kind: NoteKind?, label: String, icon: String, count: Int) -> some View {
        let isSelected = selectedKind == kind
        return Button {
            selectedKind = kind
            Haptics.selection()
        } label: {
            HStack(spacing: AppTheme.Spacing.xs) {
                Image(systemName: icon)
                    .font(.system(size: Layout.chipIconSize, weight: .medium))
                Text(label)
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
        // The `.isSelected` trait already drives VoiceOver's "selected"
        // announcement — the previous shape also appended ", selected"
        // to the label, causing a double-narrate.
        .accessibilityLabel("\(label), \(count) note\(count == 1 ? "" : "s")")
    }

    @ViewBuilder
    private var emptyState: some View {
        if searchText.isEmpty {
            ContentUnavailableView {
                Label("No notes yet", systemImage: "note.text")
            } description: {
                Text("Tap + to jot something down, or ask your agent to create a note.")
            } actions: {
                Button("Add Note") { showNewNote = true }
                    .buttonStyle(.glassProminent)
            }
            .listRowBackground(Color.clear)
        } else {
            ContentUnavailableView.search(text: searchText)
                .listRowBackground(Color.clear)
        }
    }

    @ViewBuilder
    private var noteSections: some View {
        ForEach(groupedNotes, id: \.bucket) { group in
            Section {
                ForEach(group.items) { note in
                    NoteRow(note: note, query: searchText)
                        .agentContentRowActions(
                            onEdit: { editingNote = note },
                            copyText: note.text,
                            onDelete: { store.deleteNote(note.id) }
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
        ToolbarItem(placement: .topBarTrailing) {
            Button {
                showNewNote = true
            } label: {
                Label("Add Note", systemImage: "plus")
            }
        }
        if !activeNotes.isEmpty {
            ToolbarItem(placement: .destructiveAction) {
                Button("Clear All", role: .destructive) {
                    showClearConfirm = true
                }
            }
        }
    }

    // MARK: - NoteRow

    private struct NoteRow: View {
        let note: Note
        var query: String = ""

        var body: some View {
            AgentContentRow(
                icon: iconName,
                iconColor: iconColor,
                text: note.text,
                date: note.createdAt,
                badge: note.kind == .reflection ? "reflection" : nil,
                query: query
            )
        }

        private var iconName: String {
            switch note.kind {
            case .free:         return "note.text"
            case .reflection:   return "sparkles"
            case .systemEvent:  return "gear"
            }
        }

        private var iconColor: Color {
            switch note.kind {
            case .free:         return .indigo
            case .reflection:   return .orange
            case .systemEvent:  return .secondary
            }
        }
    }

    // MARK: - Chip display helpers

    private static func chipInfo(for kind: NoteKind) -> (label: String, icon: String) {
        switch kind {
        case .free:        return ("Notes", "note.text")
        case .reflection:  return ("Reflections", "sparkles")
        case .systemEvent: return ("System", "gear")
        }
    }
}
