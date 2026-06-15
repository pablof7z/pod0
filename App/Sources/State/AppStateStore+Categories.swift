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

    // MARK: - Kernel migration (D0/D4)

    /// UserDefaults flag guarding the one-shot legacy→kernel category migration.
    private static let migrationFlagKey = "userCategoriesMigratedToKernel"

    /// One-shot migration: seed the kernel-owned `podcast_user_categories`
    /// substate from the legacy Swift `state.categories` model. Each podcast
    /// inherits the names of every legacy category it belonged to (a podcast can
    /// live in more than one), dispatched as a single
    /// `set_podcast_user_categories` op per podcast. Idempotent across launches
    /// via the `UserDefaults` flag — runs exactly once even if the legacy data
    /// persists. A no-op on fresh installs (no legacy categories).
    func migrateUserCategoriesToKernel() {
        guard !UserDefaults.standard.bool(forKey: Self.migrationFlagKey) else { return }

        // Accumulate every legacy category name per podcast (preserving order,
        // de-duplicated) so a podcast in multiple categories migrates all labels
        // in one dispatch rather than clobbering with the last one.
        var labelsByPodcast: [UUID: [String]] = [:]
        for category in state.categories {
            let label = category.name
            guard !label.isEmpty else { continue }
            for podcastUUID in category.subscriptionIDs {
                var labels = labelsByPodcast[podcastUUID] ?? []
                if !labels.contains(label) {
                    labels.append(label)
                }
                labelsByPodcast[podcastUUID] = labels
            }
        }

        for (podcastUUID, labels) in labelsByPodcast {
            kernel?.dispatch(namespace: "podcast",
                             body: [
                                 "op": "set_podcast_user_categories",
                                 "podcast_id": podcastUUID.uuidString.lowercased(),
                                 "categories": labels,
                             ])
        }

        // Set the run-once guard only AFTER every assignment has been dispatched
        // (each dispatch persists synchronously kernel-side). Setting it before
        // the loop would strand a partial migration permanently if the app
        // crashed mid-loop — the flag would already be true on next launch and
        // the remaining podcasts would never migrate. `set_podcast_user_categories`
        // is idempotent (replaces the value), so re-running after a crash is safe.
        UserDefaults.standard.set(true, forKey: Self.migrationFlagKey)
    }
}
