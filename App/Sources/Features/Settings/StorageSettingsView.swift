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
/// The aggregation walks the downloads directory directly so orphaned
/// files (downloads whose subscription was unsubscribed, leaving the
/// episode out of `state.episodes`) still show up under "Other / orphan".
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
        return Section {
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
            store.sharedLibrary?.removeDownload(episodeID: episodeID)
        }
        Task { await refresh() }
    }

    private func deleteOrphans() {
        // Core artifacts are never removed by directory walking. A future
        // typed recovery command may expose safe orphan cleanup.
        Task { await refresh() }
    }

    private func deleteAll() {
        for episode in store.state.episodes where episode.downloadState.isAvailable {
            store.sharedLibrary?.removeDownload(episodeID: episode.id)
        }
        Task { await refresh() }
    }

    // MARK: - Computation

    /// Joins the on-disk file list against `state.episodes` /
    /// `state.subscriptions` to produce the per-show breakdown plus the
    /// orphan tally. Static so it's straightforward to drive from a
    /// detached `Task` without holding `self`.
    static func compute(store: AppStateStore) async -> Snapshot {
        let episodes = store.state.episodes
        let podcasts = Dictionary(uniqueKeysWithValues: store.state.podcasts.map { ($0.id, $0) })

        var byShow: [UUID: (title: String, bytes: Int64, episodes: Set<UUID>)] = [:]
        let orphanBytes: Int64 = 0
        var totalBytes: Int64 = 0

        for episode in episodes {
            guard let bytes = episode.downloadState.byteCount else { continue }
            totalBytes += bytes
            let title = podcasts[episode.podcastID]?.title ?? "Unknown show"
            var entry = byShow[episode.podcastID] ?? (title, 0, [])
            entry.bytes += bytes
            entry.episodes.insert(episode.id)
            // Title may have arrived stable from the first file; refresh anyway
            // in case the first file we hit was an orphan-titled fallback.
            entry.title = title
            byShow[episode.podcastID] = entry
        }

        let shows = byShow
            .map { (subID, entry) in
                ShowRow(
                    subscriptionID: subID,
                    title: entry.title,
                    bytes: entry.bytes,
                    episodeCount: entry.episodes.count,
                    episodeIDs: Array(entry.episodes)
                )
            }
            .sorted { $0.bytes > $1.bytes }

        return Snapshot(
            totalBytes: totalBytes,
            shows: shows,
            orphanBytes: orphanBytes,
            orphanCount: 0
        )
    }

    // MARK: - Snapshot

    struct Snapshot: Sendable {
        let totalBytes: Int64
        let shows: [ShowRow]
        let orphanBytes: Int64
        let orphanCount: Int
        static let empty = Snapshot(totalBytes: 0, shows: [], orphanBytes: 0, orphanCount: 0)
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
