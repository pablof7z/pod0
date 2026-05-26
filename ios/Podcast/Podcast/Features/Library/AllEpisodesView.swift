import SwiftUI

// MARK: - AllEpisodesView

/// Cross-show episode firehose: every `EpisodeSummary` from every subscribed
/// `PodcastSummary` in `model.library`, sorted newest-first by `publishedAt`.
///
/// Backed entirely by the NMP kernel snapshot — no `AppStateStore`, no compat
/// types. Episode taps push an `EpisodeRoute` value; navigation destinations
/// are registered by the enclosing `LibraryView`'s `NavigationStack`.
struct AllEpisodesView: View {
    @Environment(KernelModel.self) private var model
    @State private var searchText = ""

    var body: some View {
        Group {
            if allEpisodes.isEmpty && searchText.isEmpty {
                ContentUnavailableView(
                    "No Episodes Yet",
                    systemImage: "waveform",
                    description: Text("Subscribe to a podcast to see episodes here.")
                )
            } else if filteredEpisodes.isEmpty {
                ContentUnavailableView.search(text: searchText)
            } else {
                episodeList
            }
        }
        .navigationTitle("All Episodes")
        .navigationBarTitleDisplayMode(.inline)
        .searchable(text: $searchText, prompt: "Episode or podcast")
        .refreshable {
            model.dispatch(namespace: "podcast", body: ["op": "refresh_all"])
        }
    }

    // MARK: - Episode flattening

    private struct EpisodeWithShow: Identifiable, Hashable {
        let episode: EpisodeSummary
        let podcast: PodcastSummary
        var id: String { "\(podcast.id)|\(episode.id)" }
    }

    private var allEpisodes: [EpisodeWithShow] {
        model.library
            .flatMap { podcast in
                podcast.episodes.map { EpisodeWithShow(episode: $0, podcast: podcast) }
            }
            .sorted { a, b in
                (a.episode.publishedAt ?? 0) > (b.episode.publishedAt ?? 0)
            }
    }

    private var filteredEpisodes: [EpisodeWithShow] {
        guard !searchText.isEmpty else { return allEpisodes }
        let q = searchText.lowercased()
        return allEpisodes.filter {
            $0.episode.title.lowercased().contains(q)
            || $0.podcast.title.lowercased().contains(q)
        }
    }

    // MARK: - List

    private var episodeList: some View {
        List {
            ForEach(filteredEpisodes) { item in
                NavigationLink(value: EpisodeRoute(episode: item.episode, podcast: item.podcast)) {
                    AllEpisodesRow(
                        episode: item.episode,
                        podcast: item.podcast,
                        onPlay: { play(item) }
                    )
                }
                .buttonStyle(.plain)
                .listRowSeparator(.hidden)
                .listRowInsets(EdgeInsets(
                    top: AppTheme.Spacing.xs,
                    leading: AppTheme.Spacing.lg,
                    bottom: AppTheme.Spacing.xs,
                    trailing: AppTheme.Spacing.lg
                ))
                .listRowBackground(Color(.systemBackground))
                .swipeActions(edge: .trailing, allowsFullSwipe: true) {
                    if !item.episode.played {
                        Button {
                            model.dispatch(namespace: "podcast.inbox",
                                           body: ["op": "mark_listened", "episode_id": item.episode.id])
                        } label: {
                            Label("Played", systemImage: "checkmark.circle.fill")
                        }
                        .tint(.green)
                    }
                }
                .swipeActions(edge: .leading, allowsFullSwipe: true) {
                    Button {
                        model.dispatch(namespace: "podcast",
                                       body: ["op": "star_episode", "episode_id": item.episode.id])
                    } label: {
                        Label(item.episode.starred ? "Unbookmark" : "Bookmark",
                              systemImage: item.episode.starred ? "bookmark.slash" : "bookmark")
                    }
                    .tint(item.episode.starred ? .gray : .orange)
                }
            }
        }
        .listStyle(.plain)
    }

    // MARK: - Actions

    private func play(_ item: EpisodeWithShow) {
        Haptics.medium()
        model.dispatch(
            namespace: "podcast.player",
            body: ["op": "play", "episode_id": item.episode.id]
        )
        NotificationCenter.default.post(name: .openPlayerRequested, object: nil)
    }
}

// MARK: - AllEpisodesRow

/// Single episode row for `AllEpisodesView`. Shows artwork, episode title,
/// podcast title (cross-show context), and a meta strip with duration + date.
///
/// Differs from `KernelEpisodeRow` (single-show context): shows the podcast
/// title, omits the download indicator and resume marker, and doesn't tap-play.
private struct AllEpisodesRow: View {
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
                        .lineLimit(2)
                        .foregroundStyle(episode.played ? Color.secondary : Color.primary)
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

            Button {
                onPlay()
            } label: {
                Image(systemName: "play.circle.fill")
                    .font(.title2)
                    .foregroundStyle(Color.accentColor)
                    .frame(width: 44, height: 44)
                    .contentShape(Rectangle())
            }
            // Borderless lets the button hit-test independently of the
            // surrounding NavigationLink in the List row (a plain style
            // would let the tap fall through to the row's push action).
            .buttonStyle(.borderless)
            .accessibilityLabel("Play \(episode.title)")
        }
        .padding(.vertical, AppTheme.Spacing.sm)
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
        .accessibilityLabel(accessibilityLabel)
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
