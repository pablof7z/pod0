import SwiftUI

// MARK: - DownloadsView

/// Top-level Downloads tab. Surfaces every episode across `model.library`
/// whose snapshot reports a non-nil `downloadPath`, grouped by show.
///
/// Read model: pure projection over `KernelModel.library` — no compat stores,
/// no on-disk introspection. The Rust `PodcastStore::local_path_for` stamps
/// `EpisodeSummary.download_path` once a download `Completed` report lands
/// (PR #44 — DownloadReport back-channel), so this view automatically gains
/// rows the moment a download finishes.
///
/// Actions:
///   - Tap row: `podcast.player.play { episode_id }`. Mirrors `AllEpisodesView`
///     behaviour — promoted from the play button to the whole row since the
///     row itself only represents "ready-to-play offline".
///   - Swipe-to-delete: `podcast.delete_download { episode_id }`. The Rust
///     handler removes the file and clears the store mapping; the snapshot
///     tick drops the row.
struct DownloadsView: View {
    @Environment(KernelModel.self) private var model

    var body: some View {
        NavigationStack {
            Group {
                if downloadedShows.isEmpty {
                    emptyState
                } else {
                    downloadList
                }
            }
            .navigationTitle("Downloads")
            .navigationBarTitleDisplayMode(.large)
        }
    }

    // MARK: - Snapshot projection

    /// Episodes grouped by show, in display order. `PodcastSummary` rather than
    /// `String` for the key so we can reuse artwork / title without a second
    /// lookup. Sort: shows by title (stable A→Z); episodes inside each show by
    /// `publishedAt` descending so the newest download surfaces first.
    private struct DownloadedShow: Identifiable {
        let podcast: PodcastSummary
        let episodes: [EpisodeSummary]
        var id: String { podcast.id }
    }

    private var downloadedShows: [DownloadedShow] {
        model.library
            .compactMap { podcast -> DownloadedShow? in
                let eps = podcast.episodes
                    .filter { $0.downloadPath != nil }
                    .sorted { ($0.publishedAt ?? 0) > ($1.publishedAt ?? 0) }
                guard !eps.isEmpty else { return nil }
                return DownloadedShow(podcast: podcast, episodes: eps)
            }
            .sorted { $0.podcast.title.localizedCaseInsensitiveCompare($1.podcast.title) == .orderedAscending }
    }

    // MARK: - List

    private var downloadList: some View {
        List {
            ForEach(downloadedShows) { show in
                Section {
                    ForEach(show.episodes) { ep in
                        DownloadedEpisodeRow(
                            episode: ep,
                            podcast: show.podcast,
                            onPlay: { play(ep) }
                        )
                        .listRowSeparator(.hidden)
                        .listRowInsets(EdgeInsets(
                            top: AppTheme.Spacing.xs,
                            leading: AppTheme.Spacing.lg,
                            bottom: AppTheme.Spacing.xs,
                            trailing: AppTheme.Spacing.lg
                        ))
                        .listRowBackground(Color(.systemBackground))
                        .swipeActions(edge: .trailing, allowsFullSwipe: true) {
                            Button(role: .destructive) {
                                delete(ep)
                            } label: {
                                Label("Delete", systemImage: "trash")
                            }
                        }
                    }
                } header: {
                    Text(show.podcast.title)
                        .font(AppTheme.Typography.caption)
                        .foregroundStyle(.secondary)
                        .textCase(nil)
                }
            }
        }
        .listStyle(.plain)
    }

    // MARK: - Empty state

    private var emptyState: some View {
        ContentUnavailableView(
            "No Downloaded Episodes",
            systemImage: "arrow.down.circle",
            description: Text("Episodes you download will appear here so you can listen offline.")
        )
    }

    // MARK: - Actions

    private func play(_ ep: EpisodeSummary) {
        Haptics.medium()
        model.dispatch(
            namespace: "podcast.player",
            body: ["op": "play", "episode_id": ep.id]
        )
        NotificationCenter.default.post(name: .openPlayerRequested, object: nil)
    }

    private func delete(_ ep: EpisodeSummary) {
        Haptics.warning()
        model.dispatch(
            namespace: "podcast",
            body: ["op": "delete_download", "episode_id": ep.id]
        )
    }
}

// MARK: - DownloadedEpisodeRow

/// Single row for `DownloadsView`. Renders show artwork (falls back to
/// podcast artwork when the episode has none), the episode title, the show
/// title for cross-show context, and a duration / date meta strip.
///
/// Deliberate tailored copy of `AllEpisodesRow` — duplication is preferred
/// over reaching into a sibling Library file's private types. If a third
/// caller emerges we'll lift the row into its own shared file.
private struct DownloadedEpisodeRow: View {
    let episode: EpisodeSummary
    let podcast: PodcastSummary
    let onPlay: () -> Void

    private static let thumbnailSize: CGFloat = 56

    var body: some View {
        HStack(alignment: .center, spacing: AppTheme.Spacing.md) {
            thumbnail

            VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
                HStack(spacing: AppTheme.Spacing.xs) {
                    Text(episode.title)
                        .font(AppTheme.Typography.headline)
                        .foregroundStyle(episode.played ? Color.secondary : Color.primary)
                        .lineLimit(2)
                    if episode.starred {
                        Image(systemName: "bookmark.fill")
                            .font(.caption2)
                            .foregroundStyle(.orange)
                    }
                }

                Text(podcast.title)
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)

                metaRow
            }

            Spacer()

            // Calm checkmark — every row in this view is already downloaded
            // by definition, so no per-row download affordance is needed.
            Image(systemName: "checkmark.circle.fill")
                .font(.title3)
                .foregroundStyle(.secondary)
                .frame(width: 36, height: 36)
                .accessibilityLabel("Downloaded")

            Button {
                onPlay()
            } label: {
                Image(systemName: "play.circle.fill")
                    .font(.title2)
                    .foregroundStyle(Color.accentColor)
                    .frame(width: 44, height: 44)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.borderless)
            .accessibilityLabel("Play \(episode.title)")
        }
        .padding(.vertical, AppTheme.Spacing.sm)
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
        .accessibilityLabel(accessibilityLabel)
        .accessibilityAddTraits(.isButton)
        .onTapGesture { onPlay() }
    }

    // MARK: - Artwork

    private var artworkURL: URL? {
        if let s = episode.artworkUrl, let url = URL(string: s) { return url }
        if let s = podcast.artworkUrl, let url = URL(string: s) { return url }
        return nil
    }

    @ViewBuilder
    private var thumbnail: some View {
        let shape = RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous)
        Group {
            if let url = artworkURL {
                AsyncImage(url: url) { phase in
                    switch phase {
                    case .success(let image): image.resizable().scaledToFill()
                    default: thumbnailPlaceholder
                    }
                }
            } else {
                thumbnailPlaceholder
            }
        }
        .frame(width: Self.thumbnailSize, height: Self.thumbnailSize)
        .clipShape(shape)
        .accessibilityHidden(true)
    }

    private var thumbnailPlaceholder: some View {
        ZStack {
            Color.secondary.opacity(0.18)
            Image(systemName: "waveform")
                .font(.system(size: 20, weight: .light))
                .foregroundStyle(.secondary)
        }
    }

    // MARK: - Meta

    @ViewBuilder
    private var metaRow: some View {
        let hasDuration = episode.durationSecs != nil
        let hasDate = episode.publishedAt != nil
        if hasDuration || hasDate {
            HStack(spacing: AppTheme.Spacing.sm) {
                if let secs = episode.durationSecs {
                    Text(formatDuration(secs))
                        .font(AppTheme.Typography.monoCaption)
                        .foregroundStyle(.secondary)
                }
                if hasDuration && hasDate {
                    Text("·")
                        .font(AppTheme.Typography.caption)
                        .foregroundStyle(.tertiary)
                }
                if let ts = episode.publishedAt {
                    Text(relativeDate(from: ts))
                        .font(AppTheme.Typography.caption)
                        .foregroundStyle(.secondary)
                }
            }
        }
    }

    private var accessibilityLabel: String {
        var parts = [episode.title, podcast.title]
        if let secs = episode.durationSecs { parts.append(formatDuration(secs)) }
        if let ts = episode.publishedAt { parts.append(relativeDate(from: ts)) }
        return parts.joined(separator: ", ")
    }
}
