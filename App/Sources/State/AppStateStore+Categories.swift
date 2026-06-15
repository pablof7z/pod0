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

        // Purely-additive seed from the legacy model (no reconcile set: a fresh
        // kernel has nothing to clear).
        syncUserCategoriesToKernel()

        // Set the run-once guard only AFTER every assignment has been dispatched
        // (each dispatch persists synchronously kernel-side). Setting it before
        // the loop would strand a partial migration permanently if the app
        // crashed mid-loop — the flag would already be true on next launch and
        // the remaining podcasts would never migrate. `set_podcast_user_categories`
        // is idempotent (replaces the value), so re-running after a crash is safe.
        UserDefaults.standard.set(true, forKey: Self.migrationFlagKey)
    }

    /// Mirror the user-curated category assignments held in the legacy Swift
    /// `state.categories` model into the kernel-owned `podcast_user_categories`
    /// substate (one idempotent `set_podcast_user_categories` op per podcast).
    /// This is the single bridge that keeps the kernel — which the UI now reads
    /// from (`PodcastSummary.userCategories`) — in sync with every writer of the
    /// legacy model: the one-shot launch migration AND the AI recompute path.
    /// Without it an AI re-categorization would update only the Swift copy and
    /// silently never surface in the kernel-backed UI.
    ///
    /// - Parameter reconcilingFollowed: when non-nil, every podcast in this set
    ///   that ends up with NO labels is dispatched with an empty list so the
    ///   kernel clears its now-stale assignment. Pass the authoritative followed
    ///   set after a recompute (which can drop a podcast from all categories).
    ///   When nil (the migration seed) only non-empty assignments are dispatched.
    func syncUserCategoriesToKernel(reconcilingFollowed followed: Set<UUID>? = nil) {
        // Accumulate every category name per podcast (preserving order,
        // de-duplicated) so a podcast in multiple categories carries all labels
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

        // Reconcile: clear kernel labels for followed podcasts that no longer
        // belong to any category (empty list = clear), only when an
        // authoritative set is supplied.
        if let followed {
            for podcastUUID in followed where labelsByPodcast[podcastUUID] == nil {
                labelsByPodcast[podcastUUID] = []
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
    }

    // MARK: - Transcription settings kernel migration (D4/D7)

    private static let transcriptionMigrationFlagKey = "transcriptionSettingsMigratedToKernel"

    /// One-shot migration: seed the kernel-owned per-podcast transcription
    /// disabled set from the legacy `CategorySettings.transcriptionEnabled`
    /// model. Guarded by a `UserDefaults` flag — runs exactly once; a no-op
    /// on fresh installs (all defaults are `true`, no dispatches needed).
    ///
    /// CRITICAL: flag is set AFTER the dispatch loop, never before, so a crash
    /// mid-loop retries correctly on the next launch (idempotent dispatches).
    func migrateTranscriptionSettingsToKernel() {
        guard !UserDefaults.standard.bool(forKey: Self.transcriptionMigrationFlagKey) else { return }

        syncTranscriptionSettingsToKernel()

        // Flag set AFTER dispatch loop — see migrateUserCategoriesToKernel comment.
        UserDefaults.standard.set(true, forKey: Self.transcriptionMigrationFlagKey)
    }

    /// Mirror per-category `transcriptionEnabled = false` into the kernel as
    /// per-podcast `set_podcast_transcription_enabled enabled:false` ops. Only
    /// dispatches for podcasts whose effective transcription is `false` (the
    /// non-default case), to keep the wire quiet for the typical all-enabled state.
    func syncTranscriptionSettingsToKernel() {
        for category in state.categories {
            let settings = state.categorySettings[category.id] ?? .default(for: category.id)
            guard !settings.transcriptionEnabled else { continue }
            for podcastUUID in category.subscriptionIDs {
                kernel?.dispatch(namespace: "podcast",
                                 body: [
                                     "op": "set_podcast_transcription_enabled",
                                     "podcast_id": podcastUUID.uuidString.lowercased(),
                                     "enabled": false,
                                 ])
            }
        }
    }
}
