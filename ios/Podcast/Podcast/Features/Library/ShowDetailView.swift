import SwiftUI

// MARK: - ShowDetailView

/// Show-level detail screen pushed from `LibraryView`. Reads episode list
/// directly from the embedded `PodcastSummary.episodes` — no AppStateStore.
///
/// All mutations dispatch through `KernelModel`:
///   - Pull-to-refresh  → `podcast.refresh`
///   - Unsubscribe      → `podcast.unsubscribe`
///   - Follow (from search result) → `podcast.subscribe` via feed_url
struct ShowDetailView: View {
    @Environment(KernelModel.self) private var model
    @Environment(\.dismiss) private var dismiss

    let podcast: PodcastSummary

    @State private var showUnsubscribeConfirm = false
    @State private var searchText = ""
    @State private var isSearchActive = false

    var body: some View {
        List {
            Section {
                ShowDetailHeader(podcast: livePodcast)
                    .listRowInsets(EdgeInsets())
                    .listRowSeparator(.hidden)
                    .listRowBackground(Color.clear)

                episodesHeader
                    .listRowInsets(EdgeInsets(
                        top: AppTheme.Spacing.lg,
                        leading: AppTheme.Spacing.lg,
                        bottom: AppTheme.Spacing.sm,
                        trailing: AppTheme.Spacing.lg
                    ))
                    .listRowSeparator(.hidden)
                    .listRowBackground(Color.clear)
            }
            episodeListSection
        }
        .listStyle(.plain)
        .scrollContentBackground(.hidden)
        .background { backgroundGradient }
        .navigationTitle(livePodcast.title)
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.hidden, for: .navigationBar)
        .toolbar { toolbarContent }
        .searchable(
            text: $searchText,
            isPresented: $isSearchActive,
            placement: .navigationBarDrawer(displayMode: .always),
            prompt: "Search episodes"
        )
        .refreshable {
            model.dispatch(namespace: "podcast", body: ["op": "refresh", "podcast_id": podcast.id])
        }
        .alert(
            "Unsubscribe from \(livePodcast.title)?",
            isPresented: $showUnsubscribeConfirm
        ) {
            Button("Cancel", role: .cancel) {}
            Button("Unsubscribe", role: .destructive) { performUnsubscribe() }
        } message: {
            Text("This removes the show and all of its episodes from your library.")
        }
    }

    // MARK: - Live snapshot

    /// Re-read from model.library so the UI reflects store writes (refresh).
    private var livePodcast: PodcastSummary {
        model.library.first { $0.id == podcast.id } ?? podcast
    }

    private var episodes: [EpisodeSummary] { livePodcast.episodes }

    private var filteredEpisodes: [EpisodeSummary] {
        guard !searchText.isEmpty else { return episodes }
        return episodes.filter {
            $0.title.localizedCaseInsensitiveContains(searchText)
        }
    }

    private var isInLibrary: Bool {
        model.library.contains { $0.id == podcast.id }
    }

    // MARK: - Pieces

    private var episodesHeader: some View {
        HStack {
            Text("Episodes").font(AppTheme.Typography.title)
            if !searchText.isEmpty {
                Text("\(filteredEpisodes.count) of \(episodes.count)")
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
        }
    }

    @ViewBuilder
    private var episodeListSection: some View {
        if episodes.isEmpty {
            Section {
                ContentUnavailableView(
                    "No episodes yet",
                    systemImage: "tray",
                    description: Text("Pull down to refresh this feed.")
                )
                .listRowSeparator(.hidden)
                .listRowBackground(Color.clear)
                .padding(.top, AppTheme.Spacing.xl)
            }
        } else if filteredEpisodes.isEmpty {
            Section {
                ContentUnavailableView.search(text: searchText)
                    .listRowSeparator(.hidden)
                    .listRowBackground(Color.clear)
                    .padding(.top, AppTheme.Spacing.xl)
            }
        } else {
            Section {
                ShowDetailEpisodeList(episodes: filteredEpisodes, podcast: livePodcast)
            }
        }
    }

    private var backgroundGradient: some View {
        VStack(spacing: 0) {
            LinearGradient(
                colors: [Color.accentColor.opacity(0.4), Color.accentColor.opacity(0.1), Color(.systemBackground)],
                startPoint: .top, endPoint: .bottom
            )
            .frame(height: 300)
            Color(.systemBackground)
        }
        .ignoresSafeArea()
    }

    // MARK: - Toolbar

    @ToolbarContentBuilder
    private var toolbarContent: some ToolbarContent {
        ToolbarItem(placement: .topBarTrailing) {
            Button {
                Haptics.light()
                isSearchActive = true
            } label: {
                Image(systemName: "magnifyingglass").font(.title3)
            }
            .accessibilityLabel("Search episodes")
        }
        ToolbarItem(placement: .topBarTrailing) {
            Menu {
                if isInLibrary {
                    autoDownloadToggleButton
                    Button(role: .destructive) {
                        Haptics.warning()
                        showUnsubscribeConfirm = true
                    } label: {
                        Label("Unsubscribe", systemImage: "minus.circle")
                    }
                } else if let feedUrl = podcast.feedUrl {
                    Button {
                        Haptics.light()
                        model.dispatch(namespace: "podcast", body: ["op": "subscribe", "feed_url": feedUrl])
                    } label: {
                        Label("Follow", systemImage: "plus.circle")
                    }
                }
                if let feedUrl = podcast.feedUrl, let url = URL(string: feedUrl) {
                    ShareLink(item: url, preview: SharePreview(podcast.title)) {
                        Label("Share show", systemImage: "square.and.arrow.up")
                    }
                }
            } label: {
                Image(systemName: "ellipsis.circle").font(.title3)
            }
            .accessibilityLabel("Show options")
        }
    }

    /// Menu item that toggles the per-show auto-download policy. The
    /// rendered state comes from the live snapshot
    /// (`livePodcast.autoDownload`) so a toggle dispatched here flips the
    /// next time the kernel pushes a `rev` bump. Per D0 the decision lives
    /// in Rust; iOS only renders the boolean.
    ///
    /// Note: `JSONSerialization` bridges Swift `Bool` correctly as JSON
    /// `true`/`false` via `__NSCFBoolean` (not the numeric NSNumber
    /// fast path), so a literal `Bool` in `[String: Any]` deserializes
    /// against Rust `serde::Deserialize for bool` without ceremony.
    @ViewBuilder
    private var autoDownloadToggleButton: some View {
        let nextValue = !livePodcast.autoDownload
        Button {
            Haptics.light()
            model.dispatch(
                namespace: "podcast",
                body: [
                    "op": "set_auto_download",
                    "podcast_id": podcast.id,
                    "enabled": nextValue,
                ]
            )
        } label: {
            if livePodcast.autoDownload {
                Label("Auto-download on", systemImage: "checkmark.circle.fill")
            } else {
                Label("Auto-download new episodes", systemImage: "arrow.down.circle")
            }
        }
    }

    // MARK: - Actions

    private func performUnsubscribe() {
        model.dispatch(namespace: "podcast", body: ["op": "unsubscribe", "podcast_id": podcast.id])
        dismiss()
    }
}
