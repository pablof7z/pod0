import SwiftUI

// MARK: - BookmarksView

/// Global Bookmarks screen — every episode that has been starred, clipped, or
/// annotated with a note. Rows show small chips indicating which content types
/// are present. Tap a row to open the episode detail.
struct BookmarksView: View {

    @Environment(AppStateStore.self) private var store
    @Environment(PlaybackState.self) private var playback

    @State private var searchQuery = ""
    @State private var episodeNavTarget: UUID?

    var body: some View {
        content
            .navigationTitle("Bookmarks")
            .navigationBarTitleDisplayMode(.large)
            .background(Color(.systemGroupedBackground).ignoresSafeArea())
            .searchable(text: $searchQuery, prompt: "Search bookmarks")
            .navigationDestination(item: $episodeNavTarget) { id in
                EpisodeDetailView(episodeID: id)
            }
    }

    // MARK: - Content

    @ViewBuilder
    private var content: some View {
        let all = bookmarkedEntries()
        if all.isEmpty {
            emptyState
        } else {
            let displayed = filtered(all)
            if displayed.isEmpty {
                ContentUnavailableView.search(text: searchQuery)
            } else {
                entryList(displayed)
            }
        }
    }

    // MARK: - List

    private func entryList(_ entries: [BookmarkEntry]) -> some View {
        List(entries) { entry in
            Button {
                Haptics.selection()
                episodeNavTarget = entry.episode.id
            } label: {
                BookmarkRow(entry: entry)
            }
            .buttonStyle(.plain)
            .listRowInsets(EdgeInsets(top: AppTheme.Spacing.sm, leading: AppTheme.Spacing.md, bottom: AppTheme.Spacing.sm, trailing: AppTheme.Spacing.md))
            .listRowBackground(Color.clear)
        }
        .listStyle(.plain)
        .scrollContentBackground(.hidden)
    }

    // MARK: - Empty state

    private var emptyState: some View {
        ContentUnavailableView {
            Label("No Bookmarks Yet", systemImage: "bookmark.fill")
        } description: {
            Text("Star an episode, make a clip, or add a note — it will appear here.")
        }
    }

    // MARK: - Data

    private func bookmarkedEntries() -> [BookmarkEntry] {
        let clipsByEpisode = Dictionary(grouping: store.state.clips, by: \.episodeID)
        let notesByEpisode: [UUID: [Note]] = {
            var result: [UUID: [Note]] = [:]
            for note in store.state.notes where !note.deleted {
                guard case .episode(let id, _) = note.target else { continue }
                result[id, default: []].append(note)
            }
            return result
        }()

        var entries: [BookmarkEntry] = []
        for episode in store.state.episodes {
            let clips = clipsByEpisode[episode.id] ?? []
            let notes = notesByEpisode[episode.id] ?? []
            guard episode.isStarred || !clips.isEmpty || !notes.isEmpty else { continue }
            let podcast = store.podcast(id: episode.podcastID)
            entries.append(BookmarkEntry(
                episode: episode,
                podcast: podcast,
                hasBookmark: episode.isStarred,
                clipCount: clips.count,
                noteCount: notes.count
            ))
        }
        entries.sort { $0.episode.pubDate > $1.episode.pubDate }
        return entries
    }

    private func filtered(_ entries: [BookmarkEntry]) -> [BookmarkEntry] {
        guard !searchQuery.isEmpty else { return entries }
        let q = searchQuery.lowercased()
        return entries.filter {
            $0.episode.title.lowercased().contains(q)
            || ($0.podcast?.title.lowercased().contains(q) ?? false)
        }
    }
}

// MARK: - BookmarkEntry

private struct BookmarkEntry: Identifiable {
    var id: UUID { episode.id }
    let episode: Episode
    let podcast: Podcast?
    let hasBookmark: Bool
    let clipCount: Int
    let noteCount: Int
}

// MARK: - BookmarkRow

private struct BookmarkRow: View {
    let entry: BookmarkEntry

    private static let artworkSize: CGFloat = 52

    var body: some View {
        HStack(alignment: .center, spacing: AppTheme.Spacing.md) {
            artwork
            VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
                Text(entry.episode.title)
                    .font(AppTheme.Typography.headline)
                    .foregroundStyle(.primary)
                    .lineLimit(2)
                if let podcastTitle = entry.podcast?.title {
                    Text(podcastTitle)
                        .font(AppTheme.Typography.subheadline)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                chips
            }
        }
        .padding(.vertical, AppTheme.Spacing.xs)
    }

    // MARK: - Artwork

    private var artwork: some View {
        let shape = RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous)
        return Group {
            if let url = entry.episode.imageURL ?? entry.podcast?.imageURL {
                CachedAsyncImage(
                    url: url,
                    targetSize: CGSize(width: Self.artworkSize, height: Self.artworkSize)
                ) { phase in
                    switch phase {
                    case .success(let image): image.resizable().scaledToFill()
                    default: placeholder
                    }
                }
            } else {
                placeholder
            }
        }
        .frame(width: Self.artworkSize, height: Self.artworkSize)
        .clipShape(shape)
    }

    private var placeholder: some View {
        ZStack {
            Color.secondary.opacity(0.18)
            Image(systemName: "waveform")
                .font(.system(size: 18, weight: .light))
                .foregroundStyle(.secondary)
        }
    }

    // MARK: - Content chips

    private var chips: some View {
        HStack(spacing: AppTheme.Spacing.xs) {
            if entry.hasBookmark {
                chip(icon: "bookmark.fill", color: .accentColor)
            }
            if entry.clipCount > 0 {
                chip(icon: "scissors", label: "\(entry.clipCount)", color: .secondary)
            }
            if entry.noteCount > 0 {
                chip(icon: "note.text", label: "\(entry.noteCount)", color: .secondary)
            }
        }
    }

    private func chip(icon: String, label: String? = nil, color: Color) -> some View {
        HStack(spacing: 3) {
            Image(systemName: icon)
                .font(.system(size: 10, weight: .medium))
            if let label {
                Text(label)
                    .font(AppTheme.Typography.caption)
            }
        }
        .foregroundStyle(color)
        .padding(.horizontal, 6)
        .padding(.vertical, 3)
        .background(color.opacity(0.1), in: Capsule())
    }
}
