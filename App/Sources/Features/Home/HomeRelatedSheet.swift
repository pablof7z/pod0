import SwiftUI

// MARK: - HomeRelatedSheet
//
// "Find related across my library" — long-press affordance on a featured
// episode. Runs the existing `RAGService` over the user's transcript corpus
// using the seed episode's title + chapter titles as the query.
//
// Three lenses:
//   • `topic`   — default. Cross-show pivot: dedupe by subscription, surface
//     one episode per show (the original behavior).
//   • `sources` — group by subscription, keep multiple chunks per show so
//     the user can see how each show covered the seed concept.
//
// The `Speakers` lens from the original brief was descoped: chunks carry a
// `speakerID: UUID?` foreign key, but those ids are local to a single
// transcript, so cross-episode clustering by speaker requires a global
// speaker registry the codebase doesn't have yet. Surfacing a lens that
// only matches within a single transcript would feel broken; we left it
// for follow-up rather than ship a confusing affordance.
//
// Bottom of the sheet exposes a *"Compose a wiki page from these"* button
// that opens `WikiGenerateSheet` with the seed's title prefilled as the
// topic — the same RAG corpus drives the compile, so the wiki page reflects
// what the user is reading right now.

struct HomeRelatedSheet: View {
    let seedEpisode: Episode
    let seedPodcast: Podcast?

    @Environment(AppStateStore.self) private var store
    @Environment(\.dismiss) private var dismiss
    @State private var lens: Lens = .topic
    @State private var phase: Phase = .loading
    @State private var matches: [Match] = []
    @State private var showWikiCompose: Bool = false

    enum Lens: String, CaseIterable, Identifiable {
        case topic
        case sources
        var id: String { rawValue }
        var label: String {
            switch self {
            case .topic:   return "Topic"
            case .sources: return "Sources"
            }
        }
    }

    private enum Phase: Equatable {
        case loading
        case ready
        case empty
    }

    /// One result row. Carries the matched chunk's metadata + the snippet.
    struct Match: Identifiable, Equatable {
        let id: UUID
        let episode: Episode
        let podcast: Podcast?
        let snippet: String
    }

    var body: some View {
        NavigationStack {
            content
                .navigationTitle("Related")
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .topBarTrailing) {
                        Button("Done") {
                            Haptics.selection()
                            dismiss()
                        }
                    }
                }
                .safeAreaInset(edge: .bottom) {
                    if phase == .ready {
                        composeButton
                            .padding(.horizontal, AppTheme.Spacing.md)
                            .padding(.vertical, AppTheme.Spacing.sm)
                            .background(.thinMaterial)
                    }
                }
                .task(id: lens) { await loadRelated() }
                .sheet(isPresented: $showWikiCompose) {
                    WikiGenerateSheet(
                        storage: WikiStorage.shared,
                        onCompile: { _ in
                            showWikiCompose = false
                            dismiss()
                        },
                        initialTopic: seedEpisode.title
                    )
                }
        }
    }

    // MARK: - Content

    @ViewBuilder
    private var content: some View {
        VStack(spacing: 0) {
            lensPicker
            switch phase {
            case .loading: loadingState
            case .empty:   emptyState
            case .ready:   resultsList
            }
        }
    }

    private var lensPicker: some View {
        LiquidGlassSegmentedPicker(
            "Related lens",
            selection: $lens,
            segments: Lens.allCases.map { ($0, $0.label) }
        )
        .padding(.horizontal, AppTheme.Spacing.md)
        .padding(.vertical, AppTheme.Spacing.sm)
    }

    private var loadingState: some View {
        VStack(spacing: AppTheme.Spacing.md) {
            ProgressView()
            Text("Searching your library…")
                .font(AppTheme.Typography.callout)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var emptyState: some View {
        ContentUnavailableView(
            "No related episodes",
            systemImage: "sparkle.magnifyingglass",
            description: Text("Index a few transcripts first — they power cross-show matches.")
        )
    }

    @ViewBuilder
    private var resultsList: some View {
        switch lens {
        case .topic:
            List(matches) { match in
                NavigationLink {
                    EpisodeDetailView(episodeID: match.episode.id)
                } label: {
                    HomeRelatedRow(match: match)
                }
                .simultaneousGesture(TapGesture().onEnded { Haptics.selection() })
            }
            .listStyle(.plain)
        case .sources:
            // Group by subscription so the user sees coverage breadth.
            // Within each group, rows stay in match-score order. Drops any
            // match without a subscription — those would each need their
            // own bucket (and rendering an "Unknown show" group from
            // multiple anonymous matches just feels broken).
            let attributed = matches.filter { $0.podcast != nil }
            let groups = Dictionary(grouping: attributed) { $0.podcast!.id }
            var seenKeys = Set<UUID>()
            let orderedKeys = attributed.compactMap { match -> UUID? in
                let id = match.podcast!.id
                return seenKeys.insert(id).inserted ? id : nil
            }
            List {
                ForEach(orderedKeys, id: \.self) { key in
                    if let bucket = groups[key], let first = bucket.first {
                        Section(first.podcast?.title ?? "Unknown show") {
                            ForEach(bucket) { match in
                                NavigationLink {
                                    EpisodeDetailView(episodeID: match.episode.id)
                                } label: {
                                    HomeRelatedRow(match: match, hideShowTitle: true)
                                }
                                .simultaneousGesture(TapGesture().onEnded { Haptics.selection() })
                            }
                        }
                    }
                }
            }
            .listStyle(.insetGrouped)
        }
    }

    // MARK: - Compose

    private var composeButton: some View {
        Button {
            Haptics.light()
            showWikiCompose = true
        } label: {
            Label("Compose a wiki page from these", systemImage: "doc.text.magnifyingglass")
                .font(.subheadline.weight(.semibold))
                .frame(maxWidth: .infinity)
                .padding(.vertical, AppTheme.Spacing.sm)
        }
        .buttonStyle(.borderedProminent)
        .tint(AppTheme.Tint.agentSurface)
        .accessibilityHint("Opens the wiki compose sheet with this topic prefilled.")
    }

    // MARK: - Loading

    private func loadRelated() async {
        phase = .loading
        matches = []
        let query = buildQuery()
        let viaRAG = await searchRAG(query: query)
        if !viaRAG.isEmpty {
            matches = viaRAG
            phase = .ready
            return
        }
        let viaThreading = threadingMatches()
        if !viaThreading.isEmpty {
            matches = viaThreading
            phase = .ready
            return
        }
        phase = .empty
    }

    /// Compose the retrieval query: episode title plus any non-ad chapter
    /// titles. Ad-flagged chapters (`includeInTableOfContents == false`)
    /// are intentionally skipped — their text would just bias the search
    /// toward sponsor content.
    private func buildQuery() -> String {
        var parts: [String] = [seedEpisode.title]
        if let chapters = seedEpisode.chapters {
            let titles = chapters
                .filter { $0.includeInTableOfContents }
                .map(\.title)
                .filter { !$0.isEmpty }
            parts.append(contentsOf: titles.prefix(8))
        }
        return parts.joined(separator: " · ")
    }

    private func searchRAG(query: String) async -> [Match] {
        do {
            // Sources lens wants multiple hits per show, so over-fetch and
            // skip the dedupe-by-subscription step the Topic lens applies.
            let k = (lens == .sources) ? 24 : 12
            let opts = RAGSearch.Options(k: k, hybrid: true, rerank: true)
            let chunkMatches = try await RAGService.shared.search.search(
                query: query,
                scope: nil,
                options: opts
            )
            switch lens {
            case .topic:    return collapseByShow(chunkMatches)
            case .sources:  return keepPerChunk(chunkMatches)
            }
        } catch {
            return []
        }
    }

    /// Topic lens: dedupe by subscription, drop the seed itself. Take the
    /// best-scored chunk per subscription so the sheet doesn't collapse to
    /// "the same show, three times".
    private func collapseByShow(_ chunkMatches: [ChunkMatch]) -> [Match] {
        var seenSubs: Set<UUID> = [seedEpisode.podcastID]
        var collected: [Match] = []
        for chunk in chunkMatches {
            guard let ep = store.episode(id: chunk.chunk.episodeID),
                  ep.id != seedEpisode.id,
                  !seenSubs.contains(ep.podcastID) else { continue }
            seenSubs.insert(ep.podcastID)
            collected.append(Match(
                id: ep.id,
                episode: ep,
                podcast: store.podcast(id: ep.podcastID),
                snippet: String(chunk.chunk.text.prefix(220))
            ))
            if collected.count >= 8 { break }
        }
        return collected
    }

    /// Sources lens: keep every chunk match (still drops the seed itself).
    /// Match ids must be unique for SwiftUI's `ForEach`, so we use the
    /// chunk id rather than the episode id — the same episode can produce
    /// multiple rows when more than one chunk hits.
    private func keepPerChunk(_ chunkMatches: [ChunkMatch]) -> [Match] {
        var collected: [Match] = []
        for chunk in chunkMatches {
            guard let ep = store.episode(id: chunk.chunk.episodeID),
                  ep.id != seedEpisode.id else { continue }
            collected.append(Match(
                id: chunk.chunk.id,
                episode: ep,
                podcast: store.podcast(id: ep.podcastID),
                snippet: String(chunk.chunk.text.prefix(220))
            ))
            if collected.count >= 24 { break }
        }
        return collected
    }

    /// Fallback when no transcripts are indexed yet: surface episodes
    /// from threading topics that mention the seed episode. Cheap-but-
    /// useful: the user gets *something* even before they request
    /// transcript ingestion for any show.
    private func threadingMatches() -> [Match] {
        let mentions = store.state.threadingMentions
            .filter { $0.episodeID == seedEpisode.id }
        let topicIDs = Set(mentions.map(\.topicID))
        guard !topicIDs.isEmpty else { return [] }
        var seenSubs: Set<UUID> = [seedEpisode.podcastID]
        var collected: [Match] = []
        for topicID in topicIDs {
            for mention in store.threadingMentions(forTopic: topicID) {
                guard mention.episodeID != seedEpisode.id,
                      let ep = store.episode(id: mention.episodeID),
                      (lens == .sources || !seenSubs.contains(ep.podcastID)) else { continue }
                seenSubs.insert(ep.podcastID)
                collected.append(Match(
                    id: mention.id,
                    episode: ep,
                    podcast: store.podcast(id: ep.podcastID),
                    snippet: mention.snippet
                ))
                if collected.count >= 8 { break }
            }
            if collected.count >= 8 { break }
        }
        return collected
    }
}

// MARK: - HomeRelatedRow

/// Row layout used by both lenses. The Sources lens passes `hideShowTitle`
/// because the section header already names the show; rendering it again on
/// every row would be noisy.
private struct HomeRelatedRow: View {
    let match: HomeRelatedSheet.Match
    var hideShowTitle: Bool = false

    var body: some View {
        HStack(alignment: .top, spacing: AppTheme.Spacing.md) {
            VStack(alignment: .leading, spacing: 2) {
                if !hideShowTitle, let title = match.podcast?.title, !title.isEmpty {
                    Text(title)
                        .font(AppTheme.Typography.caption)
                        .tracking(0.8)
                        .textCase(.uppercase)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                Text(match.episode.title)
                    .font(AppTheme.Typography.headline)
                    .lineLimit(2)
                if !match.snippet.isEmpty {
                    Text(match.snippet)
                        .font(AppTheme.Typography.subheadline)
                        .italic()
                        .foregroundStyle(.secondary)
                        .lineLimit(3)
                        .padding(.top, AppTheme.Spacing.xs)
                }
            }
            Spacer(minLength: 0)
        }
        .padding(.vertical, AppTheme.Spacing.xs)
    }
}
