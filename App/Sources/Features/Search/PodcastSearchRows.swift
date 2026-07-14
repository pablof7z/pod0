import SwiftUI

enum PodcastSearchDestination: Identifiable, Hashable {
    case show(UUID)
    case episode(UUID)
    case wiki(WikiPage)

    var id: String {
        switch self {
        case .show(let id): "show-\(id)"
        case .episode(let id): "episode-\(id)"
        case .wiki(let page): "wiki-\(page.id)"
        }
    }
}

struct PodcastShowSearchRow: View {
    let hit: PodcastShowSearchHit
    let query: String

    var body: some View {
        HStack(spacing: AppTheme.Spacing.md) {
            PodcastSearchArtwork(podcast: hit.podcast)
            VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
                HighlightedText(text: hit.podcast.title, query: query)
                    .font(AppTheme.Typography.headline)
                    .lineLimit(2)
                if !hit.podcast.author.isEmpty {
                    HighlightedText(text: hit.podcast.author, query: query)
                        .font(AppTheme.Typography.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }
        }
        .padding(.vertical, AppTheme.Spacing.xs)
    }
}

struct PodcastEpisodeSearchRow: View {
    let hit: PodcastEpisodeSearchHit
    let query: String

    var body: some View {
        SearchResultRow(
            icon: "play.rectangle",
            tint: hit.podcast.accentColor,
            title: hit.episode.title,
            subtitle: hit.podcast.title,
            bodyText: hit.snippet,
            footnote: hit.episode.pubDate.formatted(date: .abbreviated, time: .omitted),
            query: query
        )
    }
}

struct PodcastTranscriptSearchRow: View {
    let hit: PodcastTranscriptSearchHit
    let episode: Episode?
    let podcast: Podcast?
    let query: String

    var body: some View {
        SearchResultRow(
            icon: "text.quote",
            tint: podcast?.accentColor ?? AppTheme.Tint.agentSurface,
            title: episode?.title ?? "Episode",
            subtitle: podcast?.title ?? "Transcript",
            bodyText: hit.snippet,
            footnote: "\(formatTime(hit.chunk.startMS)) · \(String(format: "%.2f", hit.score))",
            query: query
        )
    }

    private func formatTime(_ ms: Int) -> String {
        let total = max(0, ms / 1000)
        let h = total / 3600
        let m = (total % 3600) / 60
        let s = total % 60
        if h > 0 { return "\(h):\(String(format: "%02d:%02d", m, s))" }
        return "\(m):\(String(format: "%02d", s))"
    }
}

struct PodcastWikiSearchRow: View {
    let hit: PodcastWikiSearchHit
    let query: String

    var body: some View {
        SearchResultRow(
            icon: "book.closed",
            tint: .brown,
            title: hit.page.title,
            subtitle: hit.page.kind.displayName,
            bodyText: hit.excerpt,
            footnote: "\(hit.page.citations.count) citations",
            query: query
        )
    }
}

private struct SearchResultRow: View {
    let icon: String
    let tint: Color
    let title: String
    let subtitle: String
    let bodyText: String
    let footnote: String
    let query: String

    var body: some View {
        HStack(alignment: .top, spacing: AppTheme.Spacing.md) {
            Image(systemName: icon)
                .font(.body.weight(.semibold))
                .foregroundStyle(tint)
                .frame(width: 26, height: 26)
            VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
                HighlightedText(text: title, query: query)
                    .font(AppTheme.Typography.headline)
                    .lineLimit(2)
                HStack(spacing: AppTheme.Spacing.xs) {
                    Text(subtitle).lineLimit(1)
                    Text("·")
                    Text(footnote).lineLimit(1)
                }
                .font(AppTheme.Typography.caption)
                .foregroundStyle(.secondary)
                if !bodyText.isEmpty {
                    HighlightedText(text: bodyText, query: query)
                        .font(AppTheme.Typography.subheadline)
                        .foregroundStyle(.secondary)
                        .lineLimit(3)
                }
            }
        }
        .padding(.vertical, AppTheme.Spacing.xs)
    }
}

private struct PodcastSearchArtwork: View {
    let podcast: Podcast

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: AppTheme.Corner.sm, style: .continuous)
                .fill(podcast.accentColor.opacity(0.2))
            if let url = podcast.imageURL {
                CachedAsyncImage(url: url, targetSize: CGSize(width: 64, height: 64)) { phase in
                    if case .success(let image) = phase {
                        image.resizable().scaledToFill()
                    } else {
                        placeholder
                    }
                }
            } else {
                placeholder
            }
        }
        .frame(width: 44, height: 44)
        .clipShape(RoundedRectangle(cornerRadius: AppTheme.Corner.sm, style: .continuous))
    }

    private var placeholder: some View {
        Image(systemName: podcast.artworkSymbol)
            .font(.title3)
            .foregroundStyle(podcast.accentColor)
    }
}
