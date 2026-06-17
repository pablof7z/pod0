@preconcurrency import CoreSpotlight
import Foundation
import os.log
import UniformTypeIdentifiers

/// Indexes user-visible domain objects into iOS Spotlight so they can be
/// surfaced from system search and from Siri. Tapping a result deep-links
/// back into the app via `NSUserActivity` of type `CSSearchableItemActionType`.
///
/// Strategy: a full idempotent re-index of active notes
/// driven from `AppStateStore.init` and from each mutating store method. The
/// data set is small (UI-bounded by what fits in a single user's journal),
/// so the cost of rebuilding the index per mutation is negligible compared to
/// the complexity of an incremental indexer.
///
/// This legacy indexer now owns only Swift-local notes. Podcast and episode
/// Spotlight rows are indexed from the Rust `PodcastSummary` projection by
/// `SpotlightCapability`; this indexer only clears the old Swift-owned
/// subscription/episode domains so those rows cannot diverge from Rust policy.
///
/// Each domain is fully replaced on every reindex, so soft-deleted records
/// disappear from search automatically.
enum SpotlightIndexer {
    nonisolated private static let logger = Logger.app("SpotlightIndexer")

    // MARK: - Domains

    enum Domain: String, CaseIterable {
        case notes         = "com.podcastr.spotlight.notes"
        case memories      = "com.podcastr.spotlight.memories"
        case subscriptions = "com.podcastr.spotlight.subscriptions"
        case episodes      = "com.podcastr.spotlight.episodes"
    }

    // MARK: - Identifier scheme

    private static let notePrefix         = "note:"
    private static let subscriptionPrefix = "subscription:"
    private static let episodePrefix      = "episode:"

    static func noteIdentifier(_ id: UUID)         -> String { notePrefix         + id.uuidString }
    static func subscriptionIdentifier(_ id: UUID) -> String { subscriptionPrefix + id.uuidString }
    static func episodeIdentifier(_ id: UUID)      -> String { episodePrefix      + id.uuidString }

    /// Decoded result from a Spotlight continuation activity.
    enum DeepLink: Equatable, Identifiable {
        case note(UUID)
        case subscription(UUID)
        case episode(UUID)

        /// Stable, collision-safe identifier for use with `.sheet(item:)`.
        var id: String {
            switch self {
            case .note(let uuid):         return "note:"         + uuid.uuidString
            case .subscription(let uuid): return "subscription:" + uuid.uuidString
            case .episode(let uuid):      return "episode:"      + uuid.uuidString
            }
        }
    }

    /// Parses an identifier produced by this indexer back into a `DeepLink`.
    /// Returns nil for unknown / malformed values.
    static func deepLink(from identifier: String) -> DeepLink? {
        if identifier.hasPrefix(notePrefix) {
            let raw = String(identifier.dropFirst(notePrefix.count))
            return UUID(uuidString: raw).map(DeepLink.note)
        }
        if identifier.hasPrefix(subscriptionPrefix) {
            let raw = String(identifier.dropFirst(subscriptionPrefix.count))
            return UUID(uuidString: raw).map(DeepLink.subscription)
        }
        if identifier.hasPrefix(episodePrefix) {
            let raw = String(identifier.dropFirst(episodePrefix.count))
            return UUID(uuidString: raw).map(DeepLink.episode)
        }
        return nil
    }

    /// Convenience that pulls the Spotlight identifier out of a continuation
    /// `NSUserActivity` and decodes it.
    static func deepLink(from activity: NSUserActivity) -> DeepLink? {
        guard activity.activityType == CSSearchableItemActionType,
              let id = activity.userInfo?[CSSearchableItemActivityIdentifier] as? String
        else { return nil }
        return deepLink(from: id)
    }

    // MARK: - Reindex

    /// Replaces the contents of all Spotlight domains with current state.
    /// Safe to call from any mutation site — idempotent, and the underlying
    /// `CSSearchableIndex` calls are non-blocking.
    static func reindex(state: AppState) {
        let notes = state.notes
            .filter { !$0.deleted }
            .map(makeSearchable(from:))

        let index = CSSearchableIndex.default()
        replace(domain: .notes, with: notes, in: index)
        replace(domain: .memories, with: [], in: index)
        replace(domain: .subscriptions, with: [], in: index)
        replace(domain: .episodes, with: [], in: index)
    }

    /// Idempotent "delete-then-insert" for one domain. Items can be empty —
    /// in that case the domain is just emptied.
    private static func replace(
        domain: Domain,
        with items: [CSSearchableItem],
        in index: CSSearchableIndex
    ) {
        index.deleteSearchableItems(withDomainIdentifiers: [domain.rawValue]) { error in
            if let error {
                logger.error("Failed to delete \(domain.rawValue, privacy: .public) domain: \(error, privacy: .public)")
            }
            guard !items.isEmpty else { return }
            index.indexSearchableItems(items) { error in
                if let error {
                    logger.error("Failed to index \(domain.rawValue, privacy: .public): \(error, privacy: .public)")
                }
            }
        }
    }

    /// Removes everything this app has put into Spotlight. Useful when the
    /// user clears all data.
    static func clearAll() {
        CSSearchableIndex.default().deleteSearchableItems(
            withDomainIdentifiers: Domain.allCases.map(\.rawValue)
        ) { error in
            if let error { logger.error("Failed to clear Spotlight index: \(error, privacy: .public)") }
        }
    }

    // MARK: - Builders

    private static func makeSearchable(from note: Note) -> CSSearchableItem {
        let attrs = CSSearchableItemAttributeSet(contentType: UTType.text)
        let firstLine = note.text
            .split(whereSeparator: \.isNewline)
            .first
            .map(String.init) ?? note.text
        attrs.title = firstLine
        attrs.contentDescription = note.text
        attrs.contentCreationDate = note.createdAt
        attrs.keywords = noteKeywords(for: note)

        return CSSearchableItem(
            uniqueIdentifier: noteIdentifier(note.id),
            domainIdentifier: Domain.notes.rawValue,
            attributeSet: attrs
        )
    }

    private static func noteKeywords(for note: Note) -> [String] {
        var keywords = ["note", "journal", note.kind.rawValue]
        switch note.kind {
        case .reflection: keywords.append("reflection")
        case .systemEvent: keywords.append(contentsOf: ["system", "event", "log"])
        case .free: break
        }
        return keywords
    }

}
