import SwiftUI

// MARK: - HomeView

/// Top-level Home tab. Three sections, all driven directly by the kernel
/// snapshot — no derived caches, no business logic, no @AppStorage:
///
///   * Agent Picks — `model.podcastSnapshot?.picks`, horizontal rail of
///     large cards (see `AgentPickCard`).
///   * Continue Listening — single now-playing resume row driven by
///     `PlayerState.positionSecs` (until `EpisodeSummary` projects its
///     own per-episode `position_secs`).
///   * New Episodes — flat list of the most recent unlistened episodes
///     across the library, capped to a small N for the Home density.
///
/// Tapping any episode pushes `EpisodeDetailView` via the same
/// `EpisodeRoute` the Library tab uses.
struct HomeView: View {

    @Environment(KernelModel.self) private var model

    /// Maximum episodes surfaced in the "New Episodes" section. Set to
    /// keep Home scannable; users wanting the full list go to the
    /// Library tab.
    private static let newEpisodesLimit = 12

    var body: some View {
        NavigationStack {
            scrollContent
                .navigationTitle("Home")
                .navigationDestination(for: EpisodeRoute.self) { route in
                    EpisodeDetailView(episode: route.episode, podcast: route.podcast)
                }
                .refreshable {
                    model.dispatch(namespace: "podcast", body: ["op": "refresh_all"])
                }
        }
    }

    // MARK: - Sections

    @ViewBuilder
    private var scrollContent: some View {
        if libraryIsEmpty && picks.isEmpty {
            emptyState
        } else {
            ScrollView {
                VStack(alignment: .leading, spacing: AppTheme.Spacing.lg) {
                    if !picks.isEmpty {
                        agentPicksSection
                    }
                    if let resume = resumeContext {
                        continueListeningSection(resume)
                    }
                    if !newEpisodes.isEmpty {
                        newEpisodesSection
                    }
                }
                .padding(.vertical, AppTheme.Spacing.md)
            }
        }
    }

    // MARK: - Agent Picks

    private var agentPicksSection: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
            sectionHeader(title: "Agent Picks", systemImage: "sparkles")
            ScrollView(.horizontal, showsIndicators: false) {
                LazyHStack(spacing: AppTheme.Spacing.md) {
                    ForEach(picks) { pick in
                        if let route = route(forPickEpisodeId: pick.episodeId) {
                            NavigationLink(value: route) {
                                AgentPickCard(pick: pick)
                            }
                            .buttonStyle(.plain)
                        } else {
                            AgentPickCard(pick: pick)
                        }
                    }
                }
                .padding(.horizontal, AppTheme.Spacing.md)
            }
        }
    }

    // MARK: - Continue Listening

    private func continueListeningSection(_ ctx: ResumeContext) -> some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
            sectionHeader(title: "Continue Listening", systemImage: "play.fill")
            NavigationLink(value: EpisodeRoute(episode: ctx.episode, podcast: ctx.podcast)) {
                ContinueListeningRow(
                    episode: ctx.episode,
                    podcast: ctx.podcast,
                    positionSecs: ctx.positionSecs,
                    durationSecs: ctx.durationSecs
                )
            }
            .buttonStyle(.plain)
            .padding(.horizontal, AppTheme.Spacing.md)
        }
    }

    // MARK: - New Episodes

    private var newEpisodesSection: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
            sectionHeader(title: "New Episodes", systemImage: "clock")
            VStack(spacing: AppTheme.Spacing.xs) {
                ForEach(newEpisodes, id: \.episode.id) { entry in
                    NavigationLink(value: EpisodeRoute(episode: entry.episode, podcast: entry.podcast)) {
                        newEpisodeRow(entry)
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(.horizontal, AppTheme.Spacing.md)
        }
    }

    private func newEpisodeRow(_ entry: LibraryEpisode) -> some View {
        HStack(spacing: AppTheme.Spacing.md) {
            VStack(alignment: .leading, spacing: 2) {
                Text(entry.episode.title)
                    .font(AppTheme.Typography.callout)
                    .foregroundStyle(.primary)
                    .lineLimit(2)
                Text(entry.podcast.title)
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
            Image(systemName: "chevron.right")
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(.tertiary)
        }
        .padding(.vertical, AppTheme.Spacing.sm)
        .padding(.horizontal, AppTheme.Spacing.md)
        .background(
            RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous)
                .fill(.background.secondary)
        )
    }

    // MARK: - Section header

    private func sectionHeader(title: String, systemImage: String) -> some View {
        HStack(spacing: AppTheme.Spacing.xs) {
            Image(systemName: systemImage)
                .font(.system(size: 14, weight: .semibold))
                .foregroundStyle(Color.accentColor)
            Text(title)
                .font(AppTheme.Typography.title3)
                .foregroundStyle(.primary)
        }
        .padding(.horizontal, AppTheme.Spacing.md)
    }

    // MARK: - Empty state

    private var emptyState: some View {
        ContentUnavailableView(
            "No Picks Yet",
            systemImage: "sparkles",
            description: Text("Subscribe to a show to see AI agent picks here.")
        )
    }

    // MARK: - Snapshot derivation

    private var picks: [AgentPickSummary] {
        model.podcastSnapshot?.picks ?? []
    }

    private var libraryIsEmpty: Bool {
        model.podcastSnapshot?.library.isEmpty ?? true
    }

    /// Resolve a pick's episode-id back to an `EpisodeRoute` by walking
    /// the library. Returns `nil` if the episode (or its parent show)
    /// has fallen out of the snapshot between pick computation and
    /// render — defensive, should never happen in practice.
    private func route(forPickEpisodeId episodeId: String) -> EpisodeRoute? {
        guard let snapshot = model.podcastSnapshot else { return nil }
        for podcast in snapshot.library {
            if let episode = podcast.episodes.first(where: { $0.id == episodeId }) {
                return EpisodeRoute(episode: episode, podcast: podcast)
            }
        }
        return nil
    }

    /// Single-row "Continue Listening" entry derived from the active
    /// player state. Returns `nil` when no episode is loaded or the
    /// active episode isn't in the library snapshot.
    private var resumeContext: ResumeContext? {
        guard let snapshot = model.podcastSnapshot,
              let player = snapshot.nowPlaying,
              let episodeId = player.episodeId
        else { return nil }
        for podcast in snapshot.library {
            if let episode = podcast.episodes.first(where: { $0.id == episodeId }) {
                return ResumeContext(
                    episode: episode,
                    podcast: podcast,
                    positionSecs: player.positionSecs,
                    durationSecs: player.durationSecs ?? episode.durationSecs
                )
            }
        }
        return nil
    }

    /// Newest episodes across the whole library, capped at
    /// [`newEpisodesLimit`]. Sort key is `publishedAt` descending so
    /// undated episodes (rare; legacy feeds) drop to the bottom.
    private var newEpisodes: [LibraryEpisode] {
        let snapshot = model.podcastSnapshot
        guard let library = snapshot?.library else { return [] }
        // Skip the currently-playing episode so it doesn't appear in both
        // Continue Listening and New Episodes.
        let nowPlayingId = snapshot?.nowPlaying?.episodeId
        var flat: [LibraryEpisode] = []
        for podcast in library {
            for ep in podcast.episodes where ep.id != nowPlayingId && !ep.played {
                flat.append(LibraryEpisode(episode: ep, podcast: podcast))
            }
        }
        flat.sort { ($0.episode.publishedAt ?? 0) > ($1.episode.publishedAt ?? 0) }
        if flat.count > Self.newEpisodesLimit {
            flat = Array(flat.prefix(Self.newEpisodesLimit))
        }
        return flat
    }
}

// MARK: - Local types

/// Flat (episode, owning podcast) pair derived from the snapshot. Kept
/// inside `HomeView`'s file so other features that already use
/// `EpisodeSummary` directly are not perturbed.
private struct LibraryEpisode {
    let episode: EpisodeSummary
    let podcast: PodcastSummary
}

/// All the data the Continue Listening row needs, pre-resolved from
/// `PlayerState` + the library snapshot.
private struct ResumeContext {
    let episode: EpisodeSummary
    let podcast: PodcastSummary
    let positionSecs: Double
    let durationSecs: Double?
}
