import SwiftUI

// MARK: - CategoryEpisodesView

/// Episode firehose filtered to a single AI-assigned category.
///
/// Pushed onto the Library tab's `NavigationStack` from
/// `CategoriesView`. Iterates the entire kernel snapshot library and
/// keeps every episode whose `aiCategories` contains `category`, sorted
/// newest-first by `publishedAt`. Per D7 we filter client-side off the
/// projection; the kernel's categorizer is the source of truth for the
/// labels themselves.
struct CategoryEpisodesView: View {
    @Environment(KernelModel.self) private var model
    let category: String

    var body: some View {
        Group {
            if matchingEpisodes.isEmpty {
                emptyState
            } else {
                episodeList
            }
        }
        .navigationTitle(category)
        .navigationBarTitleDisplayMode(.inline)
    }

    // MARK: - Filter + sort

    private struct EpisodeWithShow: Identifiable, Hashable {
        let episode: EpisodeSummary
        let podcast: PodcastSummary
        var id: String { "\(podcast.id)|\(episode.id)" }
    }

    private var matchingEpisodes: [EpisodeWithShow] {
        model.library
            .flatMap { podcast in
                podcast.episodes
                    .filter { $0.aiCategories.contains(category) }
                    .map { EpisodeWithShow(episode: $0, podcast: podcast) }
            }
            .sorted { ($0.episode.publishedAt ?? 0) > ($1.episode.publishedAt ?? 0) }
    }

    // MARK: - Empty + list

    private var emptyState: some View {
        ContentUnavailableView(
            "No Episodes",
            systemImage: "tag",
            description: Text("The agent hasn't tagged any episodes with \"\(category)\" yet.")
        )
    }

    private var episodeList: some View {
        List {
            ForEach(matchingEpisodes) { item in
                NavigationLink(value: EpisodeRoute(episode: item.episode, podcast: item.podcast)) {
                    CategoryEpisodeRow(episode: item.episode, podcast: item.podcast, onPlay: { play(item) })
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
            }
        }
        .listStyle(.plain)
    }

    private func play(_ item: EpisodeWithShow) {
        Haptics.medium()
        model.dispatch(
            namespace: "podcast.player",
            body: ["op": "play", "episode_id": item.episode.id]
        )
        NotificationCenter.default.post(name: .openPlayerRequested, object: nil)
    }
}

// MARK: - CategoryEpisodeRow

private struct CategoryEpisodeRow: View {
    let episode: EpisodeSummary
    let podcast: PodcastSummary
    let onPlay: () -> Void

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
            Button(action: onPlay) {
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
    }

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
                    if case .success(let image) = phase {
                        image.resizable().scaledToFill()
                    } else {
                        thumbnailPlaceholder
                    }
                }
            } else { thumbnailPlaceholder }
        }
        .frame(width: 56, height: 56)
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

    @ViewBuilder
    private var metaRow: some View {
        HStack(spacing: AppTheme.Spacing.sm) {
            if let secs = episode.durationSecs {
                Text(formatDuration(secs))
                    .font(AppTheme.Typography.monoCaption)
                    .foregroundStyle(.secondary)
            }
            if episode.durationSecs != nil && episode.publishedAt != nil {
                Text("·").font(AppTheme.Typography.caption).foregroundStyle(.tertiary)
            }
            if let ts = episode.publishedAt {
                Text(relativeDate(from: ts))
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.secondary)
            }
        }
    }

}
