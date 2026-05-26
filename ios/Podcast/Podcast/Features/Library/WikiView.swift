import SwiftUI

// MARK: - WikiView

/// Per-podcast AI wiki — list of generated articles filtered down to one
/// show. Reads `wikiArticles` directly from the kernel snapshot
/// (`KernelModel.podcastSnapshot`), so every mutation dispatched through
/// `podcast.wiki.*` flows back into the UI on the next snapshot tick
/// without any local state.
///
/// Mutations:
///   - **Generate** — `+` toolbar button presents `GenerateWikiSheet`,
///     which dispatches `podcast.wiki.generate { podcast_id, topic }`.
///   - **Delete**   — swipe-to-delete on a row dispatches
///     `podcast.wiki.delete { article_id }`.
///   - **Open**     — tapping a row pushes `WikiArticleDetailView`.
struct WikiView: View {
    @Environment(KernelModel.self) private var model

    /// The id of the podcast whose wiki articles this screen lists.
    let podcastId: String

    @State private var showGenerateSheet = false

    var body: some View {
        Group {
            if articles.isEmpty {
                emptyState
            } else {
                articleList
            }
        }
        .navigationTitle("Wiki")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar { toolbarContent }
        .sheet(isPresented: $showGenerateSheet) {
            GenerateWikiSheet(podcastId: podcastId) {
                showGenerateSheet = false
            }
        }
    }

    // MARK: - Snapshot derivation

    /// Articles scoped to the current show. Ordered newest-first using
    /// `lastUpdatedAt` so freshly generated articles surface at the top.
    private var articles: [WikiArticle] {
        let all = model.podcastSnapshot?.wikiArticles ?? []
        return all
            .filter { $0.podcastId == podcastId }
            .sorted { $0.lastUpdatedAt > $1.lastUpdatedAt }
    }

    private var podcast: PodcastSummary? {
        model.library.first { $0.id == podcastId }
    }

    // MARK: - Pieces

    private var articleList: some View {
        List {
            if let podcast {
                Section {
                    WikiPodcastHeader(podcast: podcast)
                        .listRowInsets(EdgeInsets())
                        .listRowSeparator(.hidden)
                        .listRowBackground(Color.clear)
                }
            }
            Section {
                ForEach(articles) { article in
                    NavigationLink(value: WikiArticleRoute(articleId: article.id, podcastId: podcastId)) {
                        WikiArticleRow(article: article)
                    }
                }
                .onDelete(perform: deleteRows)
            }
        }
        .listStyle(.plain)
        .navigationDestination(for: WikiArticleRoute.self) { route in
            WikiArticleDetailView(articleId: route.articleId, podcastId: route.podcastId)
        }
    }

    private var emptyState: some View {
        ContentUnavailableView {
            Label("No wiki articles yet", systemImage: "book.closed")
        } description: {
            Text("Generate an article about a topic this show has covered. Articles synthesise transcripts and web research into a quick read.")
        } actions: {
            Button {
                Haptics.light()
                showGenerateSheet = true
            } label: {
                Text("Generate Article")
                    .font(AppTheme.Typography.headline)
                    .padding(.horizontal, AppTheme.Spacing.md)
                    .padding(.vertical, AppTheme.Spacing.sm)
            }
            .buttonStyle(.borderedProminent)
        }
    }

    @ToolbarContentBuilder
    private var toolbarContent: some ToolbarContent {
        ToolbarItem(placement: .topBarTrailing) {
            Button {
                Haptics.light()
                showGenerateSheet = true
            } label: {
                Image(systemName: "plus.circle.fill")
                    .font(.title3)
            }
            .accessibilityLabel("Add wiki article")
        }
    }

    // MARK: - Actions

    private func deleteRows(at offsets: IndexSet) {
        for index in offsets {
            let article = articles[index]
            Haptics.warning()
            model.dispatch(
                namespace: "podcast.wiki",
                body: ["op": "delete", "article_id": article.id]
            )
        }
    }
}

// MARK: - WikiArticleRoute

/// Navigation value pushed onto a `NavigationStack` to open
/// `WikiArticleDetailView`. Carries both ids so the detail view can pull
/// the live article from the snapshot on every tick.
struct WikiArticleRoute: Hashable {
    let articleId: String
    let podcastId: String
}

// MARK: - WikiArticleRow

/// One row in the article list — topic title, summary excerpt, and the
/// "last updated" timestamp. Renders a small generating indicator when
/// `isGenerating == true`.
private struct WikiArticleRow: View {
    let article: WikiArticle

    var body: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
            HStack(alignment: .firstTextBaseline, spacing: AppTheme.Spacing.sm) {
                Text(article.topic)
                    .font(AppTheme.Typography.headline)
                    .lineLimit(1)
                if article.isGenerating {
                    ProgressView()
                        .controlSize(.mini)
                }
            }
            Text(article.summary)
                .font(AppTheme.Typography.subheadline)
                .foregroundStyle(.secondary)
                .lineLimit(2)
            Text(WikiArticleRow.dateLabel(unix: article.lastUpdatedAt))
                .font(AppTheme.Typography.caption)
                .foregroundStyle(.tertiary)
        }
        .padding(.vertical, AppTheme.Spacing.xs)
    }

    static func dateLabel(unix: Int) -> String {
        guard unix > 0 else { return "—" }
        return relativeDate(from: unix)
    }
}

// MARK: - WikiPodcastHeader

/// Compact header rendered above the article list — small artwork chip +
/// show title. Lighter than `ShowDetailHeader` because the wiki list is a
/// pushed sub-screen, not a hero surface.
private struct WikiPodcastHeader: View {
    let podcast: PodcastSummary

    private static let artworkSize: CGFloat = 56

    var body: some View {
        HStack(spacing: AppTheme.Spacing.md) {
            artwork
            VStack(alignment: .leading, spacing: 2) {
                Text(podcast.title)
                    .font(AppTheme.Typography.headline)
                    .lineLimit(2)
                Text("AI wiki")
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, AppTheme.Spacing.lg)
        .padding(.vertical, AppTheme.Spacing.md)
    }

    @ViewBuilder
    private var artwork: some View {
        let shape = RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous)
        if let urlStr = podcast.artworkUrl, let url = URL(string: urlStr) {
            AsyncImage(url: url) { phase in
                switch phase {
                case .success(let image): image.resizable().scaledToFill()
                default: shape.fill(Color.accentColor.opacity(0.25))
                }
            }
            .frame(width: Self.artworkSize, height: Self.artworkSize)
            .clipShape(shape)
        } else {
            shape
                .fill(Color.accentColor.opacity(0.25))
                .frame(width: Self.artworkSize, height: Self.artworkSize)
                .overlay(
                    Image(systemName: "book.closed")
                        .foregroundStyle(.white.opacity(0.8))
                )
        }
    }
}
