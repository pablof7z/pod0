import Foundation

// MARK: - Per-category settings
//
// CRUD on `state.categorySettings` plus the small set of "what does this
// subscription actually inherit?" lookups that the rest of the app needs
// without having to know the category model exists.
//
// The categorization service (`AppStateStore+Categories.swift`, owned by
// the parallel agent) will provide a richer `category(forPodcast:)`
// helper later; until that lands we inline a linear scan over
// `state.categories.subscriptionIDs` so this extension stays
// self-contained and free of duplicate-method collisions on rebase.

extension AppStateStore {

    /// Returns the persisted settings record for `id`, or a fresh default
    /// when the user hasn't touched the category yet. Read-only — the
    /// returned value is a copy.
    func categorySettings(for id: UUID) -> CategorySettings {
        state.categorySettings[id] ?? .default(for: id)
    }

    /// Mutates (or creates) the settings record for `id` in place. The
    /// closure receives the current value (or a fresh default) and writes
    /// back through the store so persistence + projections fire normally.
    func updateCategorySettings(_ id: UUID, _ block: (inout CategorySettings) -> Void) {
        var record = state.categorySettings[id] ?? .default(for: id)
        block(&record)
        state.categorySettings[id] = record
    }

    /// Returns the auto-download policy that should actually drive new-episode
    /// behaviour for `podcastID`. Resolution order:
    ///
    /// 1. The podcast's primary category override (if any).
    /// 2. The per-subscription `autoDownload` policy as it stands today.
    func effectiveAutoDownload(forPodcast podcastID: UUID) -> AutoDownloadPolicy {
        let fallback = subscription(podcastID: podcastID)?.autoDownload ?? .default
        guard let category = state.categories.first(where: { $0.subscriptionIDs.contains(podcastID) }) else {
            return fallback
        }
        let settings = state.categorySettings[category.id] ?? .default(for: category.id)
        return settings.autoDownloadOverride ?? fallback
    }

    /// True when transcription should run for episodes of `podcastID`.
    /// Defaults to `true` in every "no category info yet" path so users
    /// who haven't run the categorizer still see transcripts ingested.
    func effectiveTranscriptionEnabled(forPodcast podcastID: UUID) -> Bool {
        guard let category = state.categories.first(where: { $0.subscriptionIDs.contains(podcastID) }) else {
            return true
        }
        let settings = state.categorySettings[category.id] ?? .default(for: category.id)
        return settings.transcriptionEnabled
    }
}
