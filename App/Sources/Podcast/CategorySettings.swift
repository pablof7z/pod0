import Foundation

/// Per-category preference bundle.
///
/// Categories themselves are produced by `PodcastCategorizationService`;
/// this struct is the user-facing knob set that decides how each category
/// behaves across the rest of the app. Keyed by `categoryID` on
/// `AppState.categorySettings` so a category can be toggled independently
/// without rewriting its parent record.
///
/// Defaults are intentionally permissive so the user never sees silent
/// feature degradation.
struct CategorySettings: Codable, Sendable, Hashable {
    /// FK back to `PodcastCategory.id`.
    var categoryID: UUID

    /// Optional override of the app-default auto-download policy for every
    /// subscription assigned to this category. `nil` means inherit (we fall
    /// back to the per-subscription policy as it stands today).
    var autoDownloadOverride: AutoDownloadPolicy?

    /// Whether transcripts/notes/episodes from this category get embedded
    /// and indexed into the RAG vector store.
    var ragEnabled: Bool

    /// Whether new-episode notifications fire for shows in this category.
    var notificationsEnabled: Bool

    init(
        categoryID: UUID,
        autoDownloadOverride: AutoDownloadPolicy? = nil,
        ragEnabled: Bool = true,
        notificationsEnabled: Bool = true
    ) {
        self.categoryID = categoryID
        self.autoDownloadOverride = autoDownloadOverride
        self.ragEnabled = ragEnabled
        self.notificationsEnabled = notificationsEnabled
    }

    /// Default record for a freshly-discovered category. Caller picks the ID.
    static func `default`(for id: UUID) -> CategorySettings {
        CategorySettings(categoryID: id)
    }

    private enum CodingKeys: String, CodingKey {
        case categoryID
        case autoDownloadOverride
        case ragEnabled
        case notificationsEnabled
    }

    // Forward-compat decoding so adding a future toggle won't fail older
    // persisted snapshots.
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        categoryID = try c.decode(UUID.self, forKey: .categoryID)
        autoDownloadOverride = try c.decodeIfPresent(AutoDownloadPolicy.self, forKey: .autoDownloadOverride)
        ragEnabled = try c.decodeIfPresent(Bool.self, forKey: .ragEnabled) ?? true
        notificationsEnabled = try c.decodeIfPresent(Bool.self, forKey: .notificationsEnabled) ?? true
    }
}
