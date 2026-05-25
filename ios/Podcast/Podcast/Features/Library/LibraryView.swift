import SwiftUI

// MARK: - LibraryView

/// Top-level Library tab. Shows a podcast grid from the NMP kernel snapshot;
/// tapping a cell pushes `ShowDetailView` through the NavigationStack.
struct LibraryView: View {
    @Environment(KernelModel.self) private var model

    @State private var showAddSheet = false

    private let columns = [GridItem(.adaptive(minimum: 140), spacing: AppTheme.Spacing.md)]

    var body: some View {
        NavigationStack {
            Group {
                if model.library.isEmpty {
                    emptyState
                } else {
                    podcastGrid
                }
            }
            .navigationTitle("Library")
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button { showAddSheet = true } label: {
                        Image(systemName: "plus")
                    }
                }
            }
            .navigationDestination(for: PodcastSummary.self) { podcast in
                ShowDetailView(podcast: podcast)
            }
            .refreshable {
                model.dispatch(namespace: "podcast", body: ["op": "refresh_all"])
            }
        }
        .sheet(isPresented: $showAddSheet) {
            AddShowSheet(onDismiss: { showAddSheet = false })
        }
    }

    private var emptyState: some View {
        ContentUnavailableView(
            "No Podcasts Yet",
            systemImage: "headphones",
            description: Text("Tap + to add a show.")
        )
    }

    private var podcastGrid: some View {
        ScrollView {
            LazyVGrid(columns: columns, spacing: AppTheme.Spacing.md) {
                ForEach(model.library) { podcast in
                    NavigationLink(value: podcast) {
                        LibraryGridCell(podcast: podcast)
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(AppTheme.Spacing.md)
        }
    }
}
