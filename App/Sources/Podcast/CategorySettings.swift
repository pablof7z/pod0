import Foundation

/// Per-category preference bundle.
///
/// Categories themselves are produced by `PodcastCategorizationService`;
/// this struct is the user-facing knob set that decides how each category
/// behaves across the rest of the app. Keyed by `categoryID` on
/// `AppState.categorySettings` so a category can be toggled independently
/// without rewriting its parent record.
///
/// Defaults are intentionally permissive (transcription / RAG / wiki /
/// briefings all on) so the user never sees silent feature degradation —
/// they explicitly opt *out* per category for things like Entertainment
/// where they don't want generated summaries.
struct CategorySettings: Codable, Sendable, Hashable {
    /// FK back to `PodcastCategory.id`.
    var categoryID: UUID

    /// Optional override of the app-default auto-download policy for every
    /// subscription assigned to this category. `nil` means inherit (we fall
    /// back to the per-subscription policy as it stands today).
    var autoDownloadOverride: AutoDownloadPolicy?

    /// Whether `TranscriptIngestService` should run against episodes from
    /// shows in this category.
    var transcriptionEnabled: Bool

    /// Whether transcripts/notes/episodes from this category get embedded
    /// and indexed into the RAG vector store.
    var ragEnabled: Bool

    /// Whether per-show wikis are generated for shows in this category.
    var wikiGenerationEnabled: Bool

    /// Whether episodes from this category are eligible for inclusion in
    /// daily / weekly briefings.
    var briefingsEnabled: Bool

    /// Whether new-episode notifications fire for shows in this category.
    var notificationsEnabled: Bool

    init(
        categoryID: UUID,
        autoDownloadOverride: AutoDownloadPolicy? = nil,
        transcriptionEnabled: Bool = true,
        ragEnabled: Bool = true,
        wikiGenerationEnabled: Bool = true,
        briefingsEnabled: Bool = true,
        notificationsEnabled: Bool = true
    ) {
        self.categoryID = categoryID
        self.autoDownloadOverride = autoDownloadOverride
        self.transcriptionEnabled = transcriptionEnabled
        self.ragEnabled = ragEnabled
        self.wikiGenerationEnabled = wikiGenerationEnabled
        self.briefingsEnabled = briefingsEnabled
        self.notificationsEnabled = notificationsEnabled
    }

    /// Default record for a freshly-discovered category. Caller picks the ID.
    static func `default`(for id: UUID) -> CategorySettings {
        CategorySettings(categoryID: id)
    }

    private enum CodingKeys: String, CodingKey {
        case categoryID
        case autoDownloadOverride
        case transcriptionEnabled
        case ragEnabled
        case wikiGenerationEnabled
        case briefingsEnabled
        case notificationsEnabled
    }

    // Forward-compat decoding so adding a future toggle won't fail older
    // persisted snapshots.
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        categoryID = try c.decode(UUID.self, forKey: .categoryID)
        autoDownloadOverride = try c.decodeIfPresent(AutoDownloadPolicy.self, forKey: .autoDownloadOverride)
        transcriptionEnabled = try c.decodeIfPresent(Bool.self, forKey: .transcriptionEnabled) ?? true
        ragEnabled = try c.decodeIfPresent(Bool.self, forKey: .ragEnabled) ?? true
        wikiGenerationEnabled = try c.decodeIfPresent(Bool.self, forKey: .wikiGenerationEnabled) ?? true
        briefingsEnabled = try c.decodeIfPresent(Bool.self, forKey: .briefingsEnabled) ?? true
        notificationsEnabled = try c.decodeIfPresent(Bool.self, forKey: .notificationsEnabled) ?? true
    }
}
