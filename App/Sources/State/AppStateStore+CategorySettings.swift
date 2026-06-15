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

    // NOTE: Auto-download evaluation ("which episodes should download right
    // now, given the policy + Wi-Fi state") is owned entirely by the Rust
    // kernel (M2): `PodcastAction::SetAutoDownload { enabled, wifi_only }`
    // records the policy and `episodes_to_auto_download` / the Wi-Fi-gated
    // batch decide what actually downloads. iOS no longer resolves a policy
    // to drive downloads; it only dispatches the user's choice through
    // `kernelSetAutoDownload` and reads the current setting back from the
    // kernel snapshot for display. The former `effectiveAutoDownload(forPodcast:)`
    // resolver lived here and was already dead (no callers) once the kernel
    // took over the decision; it has been removed rather than left as a trap.

    /// True when transcription should run for episodes of `podcastID`.
    /// Prefers the kernel-owned per-podcast override (`PodcastSummary.transcriptionEnabled`)
    /// which survives library rebuilds. Falls back to the legacy category scan
    /// when no kernel snapshot is available yet.
    func effectiveTranscriptionEnabled(forPodcast podcastID: UUID) -> Bool {
        // Prefer the kernel-owned per-podcast flag (D4/D7).
        // kernel.library holds [PodcastSummary] (id: String); state.podcasts holds [Podcast] which lacks transcriptionEnabled.
        if let summary = kernel?.library.first(where: { UUID(uuidString: $0.id) == podcastID }) {
            return summary.transcriptionEnabled
        }
        // Legacy fallback: scan categories (pre-kernel path, kept for safety).
        guard let category = state.categories.first(where: { $0.subscriptionIDs.contains(podcastID) }) else {
            return true
        }
        let settings = state.categorySettings[category.id] ?? .default(for: category.id)
        return settings.transcriptionEnabled
    }
}
