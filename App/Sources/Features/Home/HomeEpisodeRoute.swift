import Foundation

// MARK: - HomeEpisodeRoute

/// Navigation value pushed onto Home's `NavigationStack` when the user picks
/// "Episode details" from a row's context menu. Home owns its own route value
/// (rather than reusing Library's) to keep the feature boundary clean —
/// `EpisodeDetailView` resolves the episode from `AppStateStore` via `episodeID`.
struct HomeEpisodeRoute: Hashable {
    let episodeID: UUID
}
