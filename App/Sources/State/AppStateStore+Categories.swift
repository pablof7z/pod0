import Foundation

// MARK: - Podcast categories

extension AppStateStore {

    /// Replaces the current set of LLM-derived categories.
    ///
    /// Single-write entry-point so the `state.didSet` save fires once per
    /// recompute, regardless of how many categories the model returned.
    func setCategories(_ categories: [PodcastCategory]) {
        state.categories = categories
    }

    /// Moves a podcast into one category and removes it from every other
    /// category. Returns false when either ID is no longer valid.
    ///
    /// `PodcastCategory.subscriptionIDs` is a legacy field name that
    /// semantically holds **podcast** IDs in the new model — the field
    /// rename is deferred to avoid Codable churn for downstream callers.
    @discardableResult
    func moveSubscription(_ podcastID: UUID, toCategory categoryID: UUID) -> Bool {
        guard state.podcasts.contains(where: { $0.id == podcastID }),
              state.categories.contains(where: { $0.id == categoryID })
        else { return false }

        var categories = state.categories
        for index in categories.indices {
            categories[index].subscriptionIDs.removeAll { $0 == podcastID }
            if categories[index].id == categoryID {
                categories[index].subscriptionIDs.append(podcastID)
            }
        }
        if categories != state.categories {
            state.categories = categories
        }
        return true
    }

    /// Returns the category with the given ID, if any.
    func category(id: UUID) -> PodcastCategory? {
        state.categories.first(where: { $0.id == id })
    }

    /// Returns the (first) category that contains the given podcast.
    func category(forPodcast podcastID: UUID) -> PodcastCategory? {
        state.categories.first(where: { $0.subscriptionIDs.contains(podcastID) })
    }
}
