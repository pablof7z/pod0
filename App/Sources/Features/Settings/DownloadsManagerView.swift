import SwiftUI
import Pod0Core

// MARK: - DownloadsManagerView

struct DownloadsManagerView: View {
    @Environment(AppStateStore.self) private var store
    @Environment(WorkflowClient.self) private var workflows
    @State private var confirmCancelActive = false
    @State private var confirmDeleteDownloaded = false

    var body: some View {
        let rows = downloadRows
        let active = activeRows(in: rows)
        let failed = failedRows(in: rows)
        let downloaded = downloadedRows(in: rows)
        List {
            summarySection(
                activeCount: active.count,
                failedCount: failed.count,
                downloadedCount: downloaded.count
            )

            if rows.isEmpty {
                emptySection
            } else {
                if !active.isEmpty {
                    activeSection(active)
                }
                if !failed.isEmpty {
                    failedSection(failed)
                }
                if !downloaded.isEmpty {
                    downloadedSection(downloaded)
                }
                actionsSection(active: active, downloaded: downloaded)
            }
        }
        .settingsListStyle()
        .navigationTitle("Downloads")
        .navigationBarTitleDisplayMode(.large)
        .workflowAttentionScope(kinds: [.download])
        .alert("Cancel active downloads?", isPresented: $confirmCancelActive) {
            Button("Keep Downloads", role: .cancel) {}
            Button("Cancel Downloads", role: .destructive) {
                cancelActiveDownloads(active)
            }
        } message: {
            Text("This stops \(countLabel(active.count, singular: "download")) currently downloading or queued episode\(active.count == 1 ? "" : "s").")
        }
        .alert("Delete downloaded episodes?", isPresented: $confirmDeleteDownloaded) {
            Button("Keep Downloads", role: .cancel) {}
            Button("Delete Downloads", role: .destructive) {
                deleteDownloadedEpisodes(downloaded)
            }
        } message: {
            Text("This removes \(countLabel(downloaded.count, singular: "downloaded episode")) from this device. Your library and playback progress are kept.")
        }
    }

    // MARK: - Sections

    private func summarySection(
        activeCount: Int,
        failedCount: Int,
        downloadedCount: Int
    ) -> some View {
        Section {
            HStack(spacing: 0) {
                DownloadsSummaryStat(
                    value: activeCount,
                    label: "Active",
                    tint: .blue
                )
                Divider().padding(.vertical, 4)
                DownloadsSummaryStat(
                    value: failedCount,
                    label: "Failed",
                    tint: .orange
                )
                Divider().padding(.vertical, 4)
                DownloadsSummaryStat(
                    value: downloadedCount,
                    label: "Saved",
                    tint: .green
                )
            }
            .frame(minHeight: 58)
        } footer: {
            Text("Background downloads continue when the app leaves the foreground. Use this screen to inspect active work, retry failures, or free downloaded files.")
        }
    }

    private var emptySection: some View {
        Section {
            ContentUnavailableView(
                "No Downloads",
                systemImage: "arrow.down.circle",
                description: Text("Download an episode from any episode row or detail screen to see it here.")
            )
            .frame(maxWidth: .infinity)
            .padding(.vertical, AppTheme.Spacing.lg)
        }
    }

    private func activeSection(_ rows: [DownloadManagerRowData]) -> some View {
        Section("Active & Queued") {
            ForEach(rows) { row in
                DownloadsManagerRow(row: row, onAction: perform)
            }
        }
    }

    private func failedSection(_ rows: [DownloadManagerRowData]) -> some View {
        Section("Failed") {
            ForEach(rows) { row in
                DownloadsManagerRow(row: row, onAction: perform)
            }
        }
    }

    private func downloadedSection(_ rows: [DownloadManagerRowData]) -> some View {
        Section("Downloaded") {
            ForEach(rows) { row in
                DownloadsManagerRow(row: row, onAction: perform)
            }
        }
    }

    @ViewBuilder
    private func actionsSection(
        active: [DownloadManagerRowData],
        downloaded: [DownloadManagerRowData]
    ) -> some View {
        if !active.isEmpty || !downloaded.isEmpty {
            Section("Bulk Actions") {
                if !active.isEmpty {
                    Button(role: .destructive) {
                        Haptics.warning()
                        confirmCancelActive = true
                    } label: {
                        Label("Cancel Active Downloads", systemImage: "xmark.circle")
                    }
                }
                if !downloaded.isEmpty {
                    Button(role: .destructive) {
                        Haptics.warning()
                        confirmDeleteDownloaded = true
                    } label: {
                        Label("Delete Downloaded Episodes", systemImage: "trash")
                    }
                }
            }
        }
    }

    // MARK: - Rows

    private var downloadRows: [DownloadManagerRowData] {
        var episodesByID = Dictionary(
            uniqueKeysWithValues: store.downloadedEpisodesView().map { ($0.id, $0) }
        )
        for episodeID in store.sharedLibrary?.downloadManagerEpisodeIDs() ?? [] {
            if let episode = store.episode(id: episodeID) {
                episodesByID[episodeID] = episode
            }
        }

        return episodesByID.values.compactMap { episode in
            guard let status = status(for: episode) else { return nil }
            let podcast = store.podcast(id: episode.podcastID)
            return DownloadManagerRowData(
                episode: episode,
                showTitle: podcast?.title ?? "Unknown show",
                showAccent: podcast?.accentColor ?? .blue,
                artworkURL: episode.imageURL ?? podcast?.imageURL,
                status: status
            )
        }
    }

    private func activeRows(in rows: [DownloadManagerRowData]) -> [DownloadManagerRowData] {
        rows
            .filter(\.status.isActive)
            .sorted { lhs, rhs in
                if lhs.status.sortRank != rhs.status.sortRank {
                    return lhs.status.sortRank < rhs.status.sortRank
                }
                return lhs.episode.pubDate > rhs.episode.pubDate
            }
    }

    private func failedRows(in rows: [DownloadManagerRowData]) -> [DownloadManagerRowData] {
        rows
            .filter(\.status.isFailed)
            .sorted { $0.episode.pubDate > $1.episode.pubDate }
    }

    private func downloadedRows(in rows: [DownloadManagerRowData]) -> [DownloadManagerRowData] {
        rows
            .filter(\.status.isDownloaded)
            .sorted { $0.episode.pubDate > $1.episode.pubDate }
    }

    private func status(for episode: Episode) -> DownloadManagerStatus? {
        if let progress = store.sharedLibrary?.downloadProgress(episodeID: episode.id) {
            return .downloading(
                progress: progress.clampedDownloadProgress,
                bytesWritten: nil,
                expectedBytes: store.sharedLibrary?.downloadExpectedBytes(episodeID: episode.id)
            )
        }
        switch episode.downloadState {
        case .downloaded(_, let byteCount):
            return .downloaded(byteCount: byteCount)
        case .notDownloaded:
            guard let workflow = store.sharedLibrary?.downloadWorkflow(episodeID: episode.id)
            else { return nil }
            switch workflow.stage {
            case .waitingForEnvironment, .requested, .retryScheduled: return .queued
            case .hostAccepted, .transferring, .staged, .removing:
                return .downloading(progress: 0, bytesWritten: nil, expectedBytes: nil)
            case .failed:
                return .failed(message: workflow.failure?.safeDetail ?? "Download failed")
            case .cancelled, .succeeded: return nil
            case .unsupported: return .failed(message: "Unsupported download state")
            }
        }
    }

    // MARK: - Actions

    private func perform(_ action: DownloadManagerAction, row: DownloadManagerRowData) {
        switch action {
        case .start, .retry:
            Haptics.light()
            store.sharedLibrary?.retryDownload(episodeID: row.id)
        case .cancel:
            Haptics.light()
            store.sharedLibrary?.cancelDownload(episodeID: row.id)
        case .delete:
            Haptics.warning()
            store.sharedLibrary?.removeDownload(episodeID: row.id)
        }
    }

    private func cancelActiveDownloads(_ rows: [DownloadManagerRowData]) {
        for row in rows {
            store.sharedLibrary?.cancelDownload(episodeID: row.id)
        }
    }

    private func deleteDownloadedEpisodes(_ rows: [DownloadManagerRowData]) {
        for row in rows {
            store.sharedLibrary?.removeDownload(episodeID: row.id)
        }
    }

    private func countLabel(_ count: Int, singular: String) -> String {
        count == 1 ? "1 \(singular)" : "\(count) \(singular)s"
    }
}
