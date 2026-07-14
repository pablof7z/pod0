import Foundation

// MARK: - AppStateStore + Threading

/// Threading topic + mention storage. Mutations route through the `state`
/// setter so the existing `didSet` persistence path picks them up without
/// extra wiring; bulk recompute paths wrap their writes in
/// `performMutationBatch` so the projections-rebuild + save side effects
/// only run once at the end.
extension AppStateStore {

    // MARK: - Topics

    /// All known topics ordered by `lastMentionedAt` descending. Topics
    /// without any recorded mention slip to the end so the list always leads
    /// with what the user just heard.
    var threadingTopics: [ThreadingTopic] {
        state.threadingTopics.sorted { lhs, rhs in
            switch (lhs.lastMentionedAt, rhs.lastMentionedAt) {
            case let (l?, r?): return l > r
            case (.some, .none): return true
            case (.none, .some): return false
            case (.none, .none): return lhs.displayName < rhs.displayName
            }
        }
    }

    /// Looks up a topic by stable id. Returns `nil` if the topic was removed
    /// since the caller captured the id.
    func threadingTopic(id: UUID) -> ThreadingTopic? {
        state.threadingTopics.first { $0.id == id }
    }

    /// Looks up a topic by slug — the dual-link target shared with the wiki
    /// layer. Slug match is case-insensitive against the canonicalised key.
    func threadingTopic(slug: String) -> ThreadingTopic? {
        let key = WikiPage.normalize(slug: slug)
        return state.threadingTopics.first { $0.slug == key }
    }

    /// Inserts or updates `topic`, keyed on `slug`. Returns the stored
    /// instance — callers that just constructed a fresh topic should adopt
    /// the returned id so subsequent mention writes target the canonical row.
    @discardableResult
    func upsertThreadingTopic(_ topic: ThreadingTopic) -> ThreadingTopic {
        var stored = state.threadingTopics
        if let idx = stored.firstIndex(where: { $0.slug == topic.slug }) {
            var merged = stored[idx]
            merged.displayName = topic.displayName
            merged.definition = topic.definition ?? merged.definition
            merged.episodeMentionCount = topic.episodeMentionCount
            merged.contradictionCount = topic.contradictionCount
            merged.lastMentionedAt = topic.lastMentionedAt ?? merged.lastMentionedAt
            stored[idx] = merged
            state.threadingTopics = stored
            return merged
        }
        stored.append(topic)
        state.threadingTopics = stored
        return topic
    }

    /// Removes a topic and every mention pointing at it. Used by the "stop
    /// surfacing this" action (UX-09 §5). Wrapped in a mutation batch so
    /// the two state writes only trigger a single persistence pass.
    func removeThreadingTopic(id: UUID) {
        performMutationBatch {
            state.threadingTopics.removeAll { $0.id == id }
            state.threadingMentions.removeAll { $0.topicID == id }
        }
    }

    // MARK: - Mentions

    /// Mentions for `topic`, ordered by episode publish date descending
    /// (newest first). Episodes the user no longer has in their library are
    /// silently filtered so the timeline never points at dead ids.
    func threadingMentions(forTopic id: UUID) -> [ThreadingMention] {
        let knownEpisodeIDs = Set(state.episodes.map(\.id))
        return state.threadingMentions
            .filter { $0.topicID == id && knownEpisodeIDs.contains($0.episodeID) }
            .sorted { lhs, rhs in
                let lhsDate = episode(id: lhs.episodeID)?.pubDate ?? .distantPast
                let rhsDate = episode(id: rhs.episodeID)?.pubDate ?? .distantPast
                if lhsDate == rhsDate { return lhs.startMS < rhs.startMS }
                return lhsDate > rhsDate
            }
    }

    /// Inserts or replaces a single mention, keyed on `id`. The store's
    /// `didSet` path persists the change.
    func upsertThreadingMention(_ mention: ThreadingMention) {
        var stored = state.threadingMentions
        if let idx = stored.firstIndex(where: { $0.id == mention.id }) {
            stored[idx] = mention
        } else {
            stored.append(mention)
        }
        state.threadingMentions = stored
    }

    /// Bulk replace of every mention belonging to `topicID`. Used by the
    /// inference recompute path so a topic's row history always reflects the
    /// latest pass without leaving stale rows behind.
    func replaceThreadingMentions(forTopic topicID: UUID, with mentions: [ThreadingMention]) {
        var stored = state.threadingMentions
        stored.removeAll { $0.topicID == topicID }
        stored.append(contentsOf: mentions)
        state.threadingMentions = stored
    }
}
