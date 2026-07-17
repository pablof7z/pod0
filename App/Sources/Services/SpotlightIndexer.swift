@preconcurrency import CoreSpotlight
import Foundation
import os.log
import UniformTypeIdentifiers

/// Indexes user-visible domain objects into iOS Spotlight so they can be
/// surfaced from system search and from Siri. Tapping a result deep-links
/// back into the app via `NSUserActivity` of type `CSSearchableItemActionType`.
///
/// Strategy: a full idempotent re-index of `activeNotes + activeMemories`
/// driven from `AppStateStore.init` and from each mutating store method. The
/// data set is small (UI-bounded by what fits in a single user's journal),
/// so the cost of rebuilding the index per mutation is negligible compared to
/// the complexity of an incremental indexer.
///
/// Index lives in two domains:
///   - `Domain.notes`    — journal-style notes (only non-deleted)
///   - `Domain.memories` — agent memories (only non-deleted)
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
    private static let memoryPrefix       = "memory:"
    private static let subscriptionPrefix = "subscription:"
    private static let episodePrefix      = "episode:"

    /// Cap on how many episodes go into the Spotlight index. The system
    /// happily takes thousands of items, but each one is a small disk write
    /// and a continuation result the user has to scroll past. 200 covers
    /// the recently-relevant tail across a typical 30-show library.
    static let maxIndexedEpisodes = 200

    static func noteIdentifier(_ id: UUID)         -> String { notePrefix         + id.uuidString }
    static func memoryIdentifier(_ id: UUID)       -> String { memoryPrefix       + id.uuidString }
    static func subscriptionIdentifier(_ id: UUID) -> String { subscriptionPrefix + id.uuidString }
    static func episodeIdentifier(_ id: UUID)      -> String { episodePrefix      + id.uuidString }

    /// Decoded result from a Spotlight continuation activity.
    enum DeepLink: Equatable, Identifiable {
        case note(UUID)
        case memory(UUID)
        case subscription(UUID)
        case episode(UUID)

        /// Stable, collision-safe identifier for use with `.sheet(item:)`.
        var id: String {
            switch self {
            case .note(let uuid):         return "note:"         + uuid.uuidString
            case .memory(let uuid):       return "memory:"       + uuid.uuidString
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
        if identifier.hasPrefix(memoryPrefix) {
            let raw = String(identifier.dropFirst(memoryPrefix.count))
            return UUID(uuidString: raw).map(DeepLink.memory)
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

        let memories = state.agentMemories
            .filter { !$0.deleted }
            .map(makeSearchable(from:))

        // Spotlight indexes followed podcasts only — synthetic / orphan
        // podcasts have no user follow row and don't belong in search.
        let followedPodcastIDs = Set(state.subscriptions.map(\.podcastID))
        let podcastsForIndex = state.podcasts.filter { followedPodcastIDs.contains($0.id) }
        let subscriptions = podcastsForIndex.map(makeSearchable(from:))

        // Bound the episode index size: the 200 most-recent unplayed
        // episodes across all subscriptions. An unplayed cap keeps already-
        // listened material from cluttering search; the 200 ceiling caps
        // worst-case index churn for users with very large libraries.
        let podcastTitles = Dictionary(
            uniqueKeysWithValues: state.podcasts.map { ($0.id, $0.title) }
        )
        let episodes = state.episodes
            .filter { !$0.played }
            .sorted { $0.pubDate > $1.pubDate }
            .prefix(maxIndexedEpisodes)
            .map { makeSearchable(from: $0, showName: podcastTitles[$0.podcastID] ?? "") }

        let index = CSSearchableIndex.default()
        replace(domain: .notes, with: notes, in: index)
        replace(domain: .memories, with: memories, in: index)
        replace(domain: .subscriptions, with: subscriptions, in: index)
        replace(domain: .episodes, with: episodes, in: index)
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

    private static func makeSearchable(from memory: AgentMemory) -> CSSearchableItem {
        let attrs = CSSearchableItemAttributeSet(contentType: UTType.text)
        attrs.title = memoryTitle(for: memory)
        attrs.contentDescription = memory.content
        attrs.contentCreationDate = memory.createdAt
        attrs.keywords = ["memory", "agent", "remember"]

        return CSSearchableItem(
            uniqueIdentifier: memoryIdentifier(memory.id),
            domainIdentifier: Domain.memories.rawValue,
            attributeSet: attrs
        )
    }

    private static func memoryTitle(for memory: AgentMemory) -> String {
        let content = memory.content.trimmed
        let sentenceEnd = content.firstIndex(where: { ".!?".contains($0) })
        if let end = sentenceEnd {
            let candidate = String(content[...end])
            if candidate.count <= 80 { return candidate }
        }
        if content.count <= 60 { return content }
        return String(content.prefix(60)) + "…"
    }

    private static func makeSearchable(from podcast: Podcast) -> CSSearchableItem {
        let attrs = CSSearchableItemAttributeSet(contentType: UTType.audio)
        attrs.title = podcast.title
        // Show notes commonly arrive as raw HTML (`<p>`, `<a href>`, …) plus
        // named or numeric entities. Spotlight renders the snippet as literal
        // text, so without this projection users were seeing
        // `<p>Hello &amp; world</p>` in search results.
        attrs.contentDescription = EpisodeShowNotesFormatter.plainText(from: podcast.description)
        if !podcast.author.isEmpty {
            attrs.artist = podcast.author
        }
        if let imageURL = podcast.imageURL {
            attrs.thumbnailURL = imageURL
        }
        attrs.contentCreationDate = podcast.discoveredAt
        attrs.keywords = subscriptionKeywords(for: podcast)

        return CSSearchableItem(
            uniqueIdentifier: subscriptionIdentifier(podcast.id),
            domainIdentifier: Domain.subscriptions.rawValue,
            attributeSet: attrs
        )
    }

    private static func subscriptionKeywords(for podcast: Podcast) -> [String] {
        var keywords = ["podcast", "subscription", "show"]
        if !podcast.author.isEmpty { keywords.append(podcast.author) }
        keywords.append(contentsOf: podcast.categories)
        return keywords
    }

    private static func makeSearchable(from episode: Episode, showName: String) -> CSSearchableItem {
        let attrs = CSSearchableItemAttributeSet(contentType: UTType.audio)
        attrs.title = episode.title
        // Same HTML / entity issue as the subscription description —
        // Spotlight expects plain text. Route through the formatter
        // so `<p>` and `&#8217;` don't leak into the search snippet.
        attrs.contentDescription = EpisodeShowNotesFormatter.plainText(from: episode.description)
        if !showName.isEmpty {
            // `album` shows under the title in the Spotlight result row, which
            // is exactly where the user expects "which podcast is this from".
            attrs.album = showName
            attrs.artist = showName
        }
        if let imageURL = episode.imageURL {
            attrs.thumbnailURL = imageURL
        }
        attrs.contentCreationDate = episode.pubDate
        if let duration = episode.duration {
            attrs.duration = NSNumber(value: duration)
        }
        attrs.keywords = ["podcast", "episode", showName].filter { !$0.isEmpty }

        return CSSearchableItem(
            uniqueIdentifier: episodeIdentifier(episode.id),
            domainIdentifier: Domain.episodes.rawValue,
            attributeSet: attrs
        )
    }
}
