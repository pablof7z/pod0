import SwiftUI

// MARK: - HomeRelatedSheet
//
// "Find related across my library" — long-press affordance on a featured
// episode. Rust owns the related-episode query and lens policy; Swift renders
// the returned rows.
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

struct HomeRelatedSheet: View {
    let seedEpisode: Episode
    let seedPodcast: Podcast?

    @Environment(AppStateStore.self) private var store
    @Environment(\.dismiss) private var dismiss
    @State private var lens: Lens = .topic
    @State private var phase: Phase = .loading
    @State private var matches: [Match] = []

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
        let id: String
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
                .task(id: lens) { await loadRelated() }
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

    // MARK: - Loading

    private func loadRelated() async {
        phase = .loading
        matches = []
        let related = await relatedRows()
        if !related.isEmpty {
            matches = related
            phase = .ready
            return
        }
        phase = .empty
    }

    private func relatedRows() async -> [Match] {
        let limit = (lens == .sources) ? 24 : 8
        do {
            let rows = try await KernelKnowledgeClient.homeRelated(
                episodeId: seedEpisode.id.uuidString,
                lens: lens.rawValue,
                limit: limit
            )
            return await MainActor.run { rows.compactMap(rowToMatch) }
        } catch {
            return []
        }
    }

    @MainActor
    private func rowToMatch(_ row: HomeRelatedKernelRow) -> Match? {
        guard let episodeID = UUID(uuidString: row.episodeId),
              let ep = store.episode(id: episodeID) else { return nil }
        return Match(
            id: row.id,
            episode: ep,
            podcast: store.podcast(id: ep.podcastID),
            snippet: row.text
        )
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
