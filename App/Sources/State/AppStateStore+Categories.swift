import Foundation
import Pod0Core

// MARK: - Podcast categories

extension AppStateStore {

    /// Replaces the current set of LLM-derived categories.
    ///
    /// Single-write entry-point so the `state.didSet` save fires once per
    /// recompute, regardless of how many categories the model returned.
    func setCategories(_ categories: [PodcastCategory]) {
        guard let client = sharedLibrary, let current = categoryProjection else {
            Self.logger.error("Blocked category replacement before Rust authority")
            return
        }
        do {
            let projection = try client.facade.replaceCategories(
                commandId: CommandId(uuid: UUID()),
                expectedRevision: current.revision,
                categories: CategoryBridge.inputs(
                    categories: categories,
                    settings: state.categorySettings
                )
            )
            guard projection.authoritative, !projection.truncated else { return }
            categoryProjection = projection
            mutateProjectionState { CategoryBridge.applying(projection, to: &$0) }
        } catch {
            Self.logger.error("Category replacement was rejected by the shared core")
        }
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

        guard let client = sharedLibrary, let current = categoryProjection else { return false }
        do {
            let projection = try client.facade.movePodcastToCategory(
                commandId: CommandId(uuid: UUID()),
                expectedRevision: current.revision,
                podcastId: PodcastId(uuid: podcastID),
                categoryId: CategoryId(uuid: categoryID)
            )
            guard projection.authoritative, !projection.truncated else { return false }
            categoryProjection = projection
            mutateProjectionState { CategoryBridge.applying(projection, to: &$0) }
        } catch {
            Self.logger.error("Category membership mutation was rejected by the shared core")
            return false
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
