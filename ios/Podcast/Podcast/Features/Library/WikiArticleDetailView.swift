import SwiftUI

// MARK: - WikiArticleDetailView

/// Full reader for a single `WikiArticle`. Re-derives the article from
/// `KernelModel.podcastSnapshot?.wikiArticles` on every render so
/// post-generation summary updates (LLM follow-up) flow in without a
/// local copy.
///
/// Layout:
///   - Topic title + show context.
///   - Generating progress indicator while `isGenerating == true`.
///   - Summary paragraphs.
///   - Source-episodes list — each row pushes `EpisodeDetailView` for the
///     referenced episode.
struct WikiArticleDetailView: View {
    @Environment(KernelModel.self) private var model

    let articleId: String
    let podcastId: String

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: AppTheme.Spacing.lg) {
                header
                if let article, article.isGenerating {
                    generatingBanner
                }
                if let article {
                    summarySection(article: article)
                    sourcesSection(article: article)
                } else {
                    missingArticleNotice
                }
            }
            .padding(.horizontal, AppTheme.Spacing.lg)
            .padding(.vertical, AppTheme.Spacing.lg)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .navigationTitle(article?.topic ?? "Article")
        .navigationBarTitleDisplayMode(.inline)
    }

    // MARK: - Snapshot derivation

    private var article: WikiArticle? {
        model.podcastSnapshot?.wikiArticles?.first(where: { $0.id == articleId })
    }

    private var podcast: PodcastSummary? {
        model.library.first { $0.id == podcastId }
    }

    /// Map episode id to its summary by scanning the embedded episode rows
    /// on the parent show. Returns `nil` when the episode is no longer in
    /// the library (e.g. the show was unsubscribed).
    private func episode(for id: String) -> EpisodeSummary? {
        podcast?.episodes.first { $0.id == id }
    }

    // MARK: - Pieces

    @ViewBuilder
    private var header: some View {
        if let article {
            VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
                Text(article.topic)
                    .font(AppTheme.Typography.title)
                    .multilineTextAlignment(.leading)
                if let podcast {
                    Text(podcast.title)
                        .font(AppTheme.Typography.subheadline)
                        .foregroundStyle(.secondary)
                }
                Text(WikiArticleDetailView.dateLabel(unix: article.lastUpdatedAt))
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.tertiary)
            }
        }
    }

    private var generatingBanner: some View {
        HStack(spacing: AppTheme.Spacing.sm) {
            ProgressView()
                .controlSize(.small)
            Text("Generating article…")
                .font(AppTheme.Typography.subheadline)
                .foregroundStyle(.secondary)
            Spacer()
        }
        .padding(AppTheme.Spacing.md)
        .background(
            RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous)
                .fill(Color.accentColor.opacity(0.12))
        )
    }

    private func summarySection(article: WikiArticle) -> some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
            Text("Summary")
                .font(AppTheme.Typography.caption.weight(.semibold))
                .foregroundStyle(.secondary)
            ForEach(paragraphs(in: article.summary), id: \.self) { paragraph in
                Text(paragraph)
                    .font(AppTheme.Typography.body)
                    .multilineTextAlignment(.leading)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    @ViewBuilder
    private func sourcesSection(article: WikiArticle) -> some View {
        if let ids = article.sourceEpisodeIds, !ids.isEmpty {
            VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
                Text("Source episodes")
                    .font(AppTheme.Typography.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
                VStack(spacing: AppTheme.Spacing.xs) {
                    ForEach(ids, id: \.self) { episodeId in
                        sourceRow(for: episodeId)
                    }
                }
            }
        }
    }

    @ViewBuilder
    private func sourceRow(for episodeId: String) -> some View {
        if let resolved = episode(for: episodeId), let podcast {
            NavigationLink(value: EpisodeRoute(episode: resolved, podcast: podcast)) {
                HStack(spacing: AppTheme.Spacing.sm) {
                    Image(systemName: "headphones")
                        .foregroundStyle(.secondary)
                    Text(resolved.title)
                        .font(AppTheme.Typography.subheadline)
                        .foregroundStyle(.primary)
                        .lineLimit(2)
                    Spacer(minLength: 0)
                    Image(systemName: "chevron.right")
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                }
                .padding(AppTheme.Spacing.md)
                .background(
                    RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous)
                        .fill(Color.secondary.opacity(0.08))
                )
            }
            .buttonStyle(.plain)
        } else {
            HStack(spacing: AppTheme.Spacing.sm) {
                Image(systemName: "headphones.slash")
                    .foregroundStyle(.tertiary)
                Text("Episode unavailable")
                    .font(AppTheme.Typography.subheadline)
                    .foregroundStyle(.tertiary)
                Spacer()
            }
            .padding(AppTheme.Spacing.md)
            .background(
                RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous)
                    .fill(Color.secondary.opacity(0.04))
            )
        }
    }

    private var missingArticleNotice: some View {
        ContentUnavailableView(
            "Article not available",
            systemImage: "questionmark.folder",
            description: Text("This article was deleted or could not be loaded.")
        )
        .padding(.top, AppTheme.Spacing.xl)
    }

    // MARK: - Helpers

    private func paragraphs(in text: String) -> [String] {
        text.components(separatedBy: "\n\n").map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }.filter { !$0.isEmpty }
    }

    static func dateLabel(unix: Int) -> String {
        guard unix > 0 else { return "" }
        let date = Date(timeIntervalSince1970: TimeInterval(unix))
        let formatter = DateFormatter()
        formatter.dateStyle = .medium
        formatter.timeStyle = .short
        return "Updated \(formatter.string(from: date))"
    }
}
