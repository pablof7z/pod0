import SwiftUI

// MARK: - StorageSettingsView

/// Surfaces what the app is keeping on disk — total downloaded size, the
/// per-show breakdown, and a single source-of-truth for the auto-cleanup
/// lifecycle. Answers three user questions in one place:
///
///   • "Are episodes downloaded?" — yes, lists them
///   • "When are they deleted?"   — never, unless the user toggles
///                                  "Delete after played" or hits the
///                                  destructive footer
///   • "How much disk?"           — total + per-show, biggest first
///
/// Swift enumerates the downloads directory as a native file-system
/// capability, then sends those raw file facts to Rust. Rust owns the join
/// against the podcast library, orphan classification, totals, grouping, and
/// ordering so the UI stays a renderer.
struct StorageSettingsView: View {

    @Environment(AppStateStore.self) private var store

    @State private var snapshot: Snapshot = .empty
    @State private var isComputing: Bool = false
    @State private var confirmDeleteAll: Bool = false

    var body: some View {
        List {
            summarySection
            lifecycleSection
            if !snapshot.shows.isEmpty {
                breakdownSection
            }
            if snapshot.orphanBytes > 0 {
                orphanSection
            }
            destructiveSection
        }
        .settingsListStyle()
        .navigationTitle("Storage")
        .navigationBarTitleDisplayMode(.large)
        .task { await refresh() }
        .refreshable { await refresh() }
        // `.alert` instead of `.confirmationDialog` because iOS 26 promotes
        // dialogs anchored close to a tappable element (the trash button)
        // into popovers and elides the `role: .cancel` button — leaving
        // the user staring at a single red "Delete All Downloads" with no
        // visible escape. Same trap as the unsubscribe confirmation in
        // `ShowDetailView`. `.alert` reliably renders both buttons as a
        // centred modal regardless of layout context.
        .alert(
            "Delete every downloaded episode?",
            isPresented: $confirmDeleteAll
        ) {
            Button("Cancel", role: .cancel) {}
            Button("Delete All Downloads", role: .destructive) {
                Haptics.warning()
                deleteAll()
            }
        } message: {
            Text("Frees \(formattedSize(snapshot.totalBytes)) on this device. Your subscription list and playback positions are kept.")
        }
    }

    // MARK: - Sections

    private var summarySection: some View {
        Section {
            HStack {
                Image(systemName: "internaldrive.fill")
                    .font(.system(size: 24, weight: .regular))
                    .foregroundStyle(.tint)
                    .frame(width: 36)
                VStack(alignment: .leading, spacing: 2) {
                    Text(formattedSize(snapshot.totalBytes))
                        .font(.title2.weight(.semibold))
                        .monospacedDigit()
                        .contentTransition(.numericText())
                    Text(summarySubtitle)
                        .font(AppTheme.Typography.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                if isComputing {
                    ProgressView().controlSize(.small)
                }
            }
            .padding(.vertical, 4)
        } header: {
            Text("Downloads")
        } footer: {
            if snapshot.totalBytes == 0 && !isComputing {
                Text("No episodes downloaded. Tap a download icon on any episode row to fetch it for offline playback.")
            }
        }
    }

    private var lifecycleSection: some View {
        Section {
            Toggle(
                "Delete after played",
                isOn: Binding(
                    get: { store.state.settings.autoDeleteDownloadsAfterPlayed },
                    set: { enabled in
                        var settings = store.state.settings
                        settings.autoDeleteDownloadsAfterPlayed = enabled
                        store.updateSettings(settings)
                    }
                )
            )
        } header: {
            Text("Lifecycle")
        } footer: {
            if store.state.settings.autoDeleteDownloadsAfterPlayed {
                Text("Downloads are removed automatically the moment an episode is marked as played.")
            } else {
                Text("Downloads are kept on this device until you remove them. Toggle on to free space automatically as you finish listening.")
            }
        }
    }

    private var breakdownSection: some View {
        Section("By show") {
            ForEach(snapshot.shows) { row in
                HStack(spacing: AppTheme.Spacing.sm) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(row.title)
                            .font(AppTheme.Typography.body)
                            .lineLimit(1)
                        Text(row.episodeCount == 1 ? "1 episode" : "\(row.episodeCount) episodes")
                            .font(AppTheme.Typography.caption)
                            .foregroundStyle(.secondary)
                    }
                    Spacer()
                    Text(formattedSize(row.bytes))
                        .font(AppTheme.Typography.body.monospacedDigit())
                        .foregroundStyle(.secondary)
                }
                .swipeActions(edge: .trailing, allowsFullSwipe: false) {
                    Button(role: .destructive) {
                        Haptics.warning()
                        deleteShow(row)
                    } label: {
                        Label("Delete", systemImage: "trash")
                    }
                }
            }
        }
    }

    private var orphanSection: some View {
        Section {
            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text("Other")
                        .font(AppTheme.Typography.body)
                    Text(snapshot.orphanCount == 1 ? "1 stranded file" : "\(snapshot.orphanCount) stranded files")
                        .font(AppTheme.Typography.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Text(formattedSize(snapshot.orphanBytes))
                    .font(AppTheme.Typography.body.monospacedDigit())
                    .foregroundStyle(.secondary)
            }
            .swipeActions(edge: .trailing, allowsFullSwipe: false) {
                Button(role: .destructive) {
                    Haptics.warning()
                    deleteOrphans()
                } label: {
                    Label("Delete", systemImage: "trash")
                }
            }
        } footer: {
            // Footer used to read "Tap below to clean up" — but the only
            // "below" action was Delete All Downloads, which wipes
            // everything (including subscribed shows). Swipe-to-delete on
            // this row keeps the cleanup scoped to just the orphans.
            Text("Files left behind when their episode was removed (e.g. unsubscribed shows). Swipe to clean up.")
        }
    }

    private var destructiveSection: some View {
        Section {
            Button(role: .destructive) {
                Haptics.selection()
                confirmDeleteAll = true
            } label: {
                Label("Delete All Downloads", systemImage: "trash")
            }
            .disabled(snapshot.totalBytes == 0)
        }
    }

    // MARK: - Derived

    private var summarySubtitle: String {
        if isComputing && snapshot.totalBytes == 0 { return "Calculating…" }
        let total = snapshot.shows.reduce(0) { $0 + $1.episodeCount } + snapshot.orphanCount
        if total == 0 { return "Nothing on disk" }
        return total == 1 ? "1 file" : "\(total) files"
    }

    private func formattedSize(_ bytes: Int64) -> String {
        bytes.formattedFileSize
    }

    // MARK: - Actions

    private func refresh() async {
        isComputing = true
        defer { isComputing = false }
        let computed = await Task.detached(priority: .userInitiated) {
            await Self.compute(store: store)
        }.value
        await MainActor.run {
            withAnimation(.easeInOut(duration: 0.18)) {
                self.snapshot = computed
            }
        }
    }

    private func deleteShow(_ row: ShowRow) {
        for episodeID in row.episodeIDs {
            store.kernelDeleteDownload(episodeID)
        }
        Task { await refresh() }
    }

    /// Removes only files Rust classified as orphaned in the current storage
    /// snapshot. Tracked downloads are untouched.
    private func deleteOrphans() {
        for url in snapshot.orphanURLs {
            try? FileManager.default.removeItem(at: url)
        }
        Task { await refresh() }
    }

    private func deleteAll() {
        for row in snapshot.shows {
            for episodeID in row.episodeIDs {
                store.kernelDeleteDownload(episodeID)
            }
        }
        for url in snapshot.orphanURLs {
            try? FileManager.default.removeItem(at: url)
        }
        Task { await refresh() }
    }

    // MARK: - Computation

    /// Enumerates local files as an OS capability and asks Rust to produce the
    /// semantic storage snapshot.
    static func compute(store: AppStateStore) async -> Snapshot {
        let files = EpisodeDownloadStore.shared.enumerateOnDisk()
        guard !files.isEmpty else { return .empty }
        return store.rustStorageBreakdown(files: files)
    }

    // MARK: - Snapshot

    struct Snapshot: Sendable {
        let totalBytes: Int64
        let shows: [ShowRow]
        let orphanBytes: Int64
        let orphanCount: Int
        let orphanURLs: [URL]
        static let empty = Snapshot(
            totalBytes: 0,
            shows: [],
            orphanBytes: 0,
            orphanCount: 0,
            orphanURLs: []
        )
    }

    struct ShowRow: Identifiable, Sendable {
        let subscriptionID: UUID
        let title: String
        let bytes: Int64
        let episodeCount: Int
        let episodeIDs: [UUID]
        var id: UUID { subscriptionID }
    }
}
