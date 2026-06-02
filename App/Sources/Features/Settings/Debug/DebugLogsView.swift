import SwiftUI

// MARK: - DebugLogsView
//
// Settings → Debug → View logs. Renders the in-memory `DiagnosticLog` ring
// buffer newest-first with search, clipboard export, and a destructive clear.
// The per-row layout lives in `DebugLogRow`.

struct DebugLogsView: View {
    @State private var log = DiagnosticLog.shared
    @State private var searchText = ""
    @State private var showClearConfirmation = false

    var body: some View {
        content
            .navigationTitle("Logs")
            .navigationBarTitleDisplayMode(.inline)
            .searchable(text: $searchText, prompt: "Search messages")
            .toolbar { toolbarContent }
            .confirmationDialog(
                "Clear all logs?",
                isPresented: $showClearConfirmation,
                titleVisibility: .visible
            ) {
                Button("Clear Logs", role: .destructive) {
                    log.clear()
                    Haptics.success()
                }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text("This removes every captured entry. It cannot be undone.")
            }
    }

    // MARK: - Content

    @ViewBuilder
    private var content: some View {
        if log.entries.isEmpty {
            ContentUnavailableView(
                "No logs",
                systemImage: "doc.text",
                description: Text("Enable debug logging in Settings > Debug.")
            )
        } else if filteredEntries.isEmpty {
            ContentUnavailableView.search(text: searchText)
        } else {
            List(filteredEntries) { entry in
                DebugLogRow(entry: entry)
            }
            .listStyle(.plain)
        }
    }

    // MARK: - Toolbar

    @ToolbarContentBuilder
    private var toolbarContent: some ToolbarContent {
        ToolbarItemGroup(placement: .topBarTrailing) {
            Button {
                UIPasteboard.general.string = log.plainText()
                Haptics.success()
            } label: {
                Label("Copy All", systemImage: "doc.on.doc")
            }
            .disabled(log.entries.isEmpty)

            Button(role: .destructive) {
                showClearConfirmation = true
            } label: {
                Label("Clear", systemImage: "trash")
            }
            .disabled(log.entries.isEmpty)
        }
    }

    // MARK: - Derived

    /// Entries newest-first, filtered by the search query against the
    /// message body (case-insensitive). Empty query returns everything.
    private var filteredEntries: [DiagnosticEntry] {
        let reversed = log.entries.reversed()
        let trimmed = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return Array(reversed) }
        return reversed.filter {
            $0.message.localizedCaseInsensitiveContains(trimmed)
        }
    }
}
