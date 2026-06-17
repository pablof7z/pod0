import Foundation

// MARK: - Podcast categories

extension AppStateStore {

    private struct CategoryAssignmentPlan: Decodable {
        let assignments: [CategoryAssignment]
        let error: String?
    }

    private struct CategoryAssignment: Decodable {
        let podcastID: String
        let categories: [String]

        enum CodingKeys: String, CodingKey {
            case podcastID = "podcast_id"
            case categories
        }
    }

    private struct CategoryTranscriptionPlan: Decodable {
        let podcastIDs: [String]
        let error: String?

        enum CodingKeys: String, CodingKey {
            case podcastIDs = "podcast_ids"
            case error
        }
    }

    /// Replaces the current set of LLM-derived categories.
    ///
    /// Single-write entry-point so the `state.didSet` save fires once per
    /// recompute, regardless of how many categories the model returned.
    func setCategories(_ categories: [PodcastCategory]) {
        state.categories = categories
    }

    /// Returns the category with the given ID, if any.
    func category(id: UUID) -> PodcastCategory? {
        state.categories.first(where: { $0.id == id })
    }

    // MARK: - Kernel migration (D0/D4)

    /// UserDefaults flag guarding the one-shot legacy→kernel category migration.
    private static let migrationFlagKey = "userCategoriesMigratedToKernel"

    /// One-shot migration: seed the kernel-owned `podcast_user_categories`
    /// substate from the legacy Swift `state.categories` model. Swift passes the
    /// raw legacy category rows to Rust; Rust owns label expansion, de-duping,
    /// reconcile-clears, and assignment shape. Idempotent across launches via
    /// the `UserDefaults` flag — runs exactly once even if the legacy data
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

    /// Mirror user-curated category assignments from the legacy Swift
    /// `state.categories` DTOs into the kernel-owned
    /// `podcast_user_categories` substate. Swift only serializes raw rows and
    /// dispatches the Rust-planned per-podcast mutations.
    ///
    /// - Parameter reconcilingFollowed: when non-nil, every podcast in this set
    ///   that ends up with NO labels is dispatched with an empty list so the
    ///   kernel clears its now-stale assignment. Pass the authoritative followed
    ///   set after a recompute (which can drop a podcast from all categories).
    ///   When nil (the migration seed) only non-empty assignments are dispatched.
    func syncUserCategoriesToKernel(reconcilingFollowed followed: Set<UUID>? = nil) {
        guard let plan = categoryAssignmentsPlan(reconcilingFollowed: followed) else { return }
        for assignment in plan.assignments {
            kernel?.dispatch(namespace: "podcast",
                             body: [
                                 "op": "set_podcast_user_categories",
                                 "podcast_id": assignment.podcastID,
                                 "categories": assignment.categories,
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

    /// Mirror legacy per-category `transcriptionEnabled = false` facts into the
    /// kernel. Rust owns expansion from raw category/settings rows to per-podcast
    /// disabled mutations; Swift only dispatches the returned ids.
    func syncTranscriptionSettingsToKernel() {
        guard let plan = categoryTranscriptionDisabledPlan() else { return }
        for podcastID in plan.podcastIDs {
            kernel?.dispatch(namespace: "podcast",
                             body: [
                                 "op": "set_podcast_transcription_enabled",
                                 "podcast_id": podcastID,
                                 "enabled": false,
                             ])
        }
    }

    private func categoryAssignmentsPlan(reconcilingFollowed followed: Set<UUID>?) -> CategoryAssignmentPlan? {
        var payload: [String: Any] = [
            "categories": categoryObjects(),
        ]
        if let followed {
            payload["followed_podcast_ids"] = followed.map { $0.uuidString.lowercased() }
        }
        return categoryTool(CategoryAssignmentPlan.self, op: "category_assignments_plan", payload: payload)
    }

    private func categoryTranscriptionDisabledPlan() -> CategoryTranscriptionPlan? {
        categoryTool(
            CategoryTranscriptionPlan.self,
            op: "category_transcription_disabled_plan",
            payload: [
                "categories": categoryObjects(),
                "settings": state.categorySettings.map { id, settings in
                    [
                        "category_id": id.uuidString.lowercased(),
                        "transcription_enabled": settings.transcriptionEnabled,
                    ]
                },
            ]
        )
    }

    private func categoryObjects() -> [[String: Any]] {
        state.categories.map { category in
            [
                "id": category.id.uuidString.lowercased(),
                "name": category.name,
                "podcast_ids": category.subscriptionIDs.map { $0.uuidString.lowercased() },
            ]
        }
    }

    private func categoryTool<T: Decodable>(
        _ type: T.Type,
        op: String,
        payload: [String: Any]
    ) -> T? {
        guard let handle = kernel?.podcastHandlePointer else { return nil }
        var request = payload
        request["op"] = op
        guard let data = try? JSONSerialization.data(withJSONObject: request),
              let json = String(data: data, encoding: .utf8)
        else { return nil }
        let envelope = json.withCString { ptr -> String? in
            guard let result = nmp_app_podcast_agent_action_tool(handle, ptr) else {
                return nil
            }
            defer { nmp_free_string(result) }
            return String(cString: result)
        }
        guard let envelope,
              let responseData = envelope.data(using: .utf8)
        else { return nil }
        return try? JSONDecoder().decode(T.self, from: responseData)
    }
}
