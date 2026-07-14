import Foundation
import Observation

// MARK: - Threading inference service

/// Owns the cross-episode threading pipeline: turning the user's transcript
/// corpus into a set of `ThreadingTopic`s and a flat list of per-episode
/// `ThreadingMention`s the timeline view renders.
///
/// v1 is intentionally thin — the heavy lifting (semantic clustering over
/// `RAGService`, contradiction detection) is stubbed out so the surfaces can
/// be wired and reviewed end-to-end without blocking on the full pipeline.
/// The public API here matches what the views and the eventual real
/// recompute will both call:
///
/// - `recompute(store:)` — re-derive every topic from the corpus.
/// - `mentions(forTopic:)` — read-side accessor for the timeline view.
/// - `ensureTopic(slug:displayName:)` — idempotent get-or-create used by
///   deep-links from the wiki and from the in-episode agent.
///
/// The store reference is wired through `attach(store:)` (mirroring the
/// `RAGService.attach(appStore:)` pattern) so views and the eventual real
/// recompute can call the read/write API without threading the store
/// through every call site. The view layer also calls
/// `seedMockIfEmpty(store:)` on first appearance — a debug-only path that
/// injects a small fixture so the UI is reviewable on a fresh install.
/// Production state is never touched once topics exist.
@MainActor
@Observable
final class ThreadingInferenceService {

    // MARK: Singleton

    /// Process-wide handle. Mirrors `RAGService.shared` / `WikiStorage.shared`
    /// so views can reach the service without dependency injection.
    static let shared = ThreadingInferenceService()

    // MARK: State

    /// `true` while `recompute(store:)` is in flight. Drives the inline
    /// progress indicator on the topic list.
    private(set) var isRecomputing: Bool = false

    /// Last error surfaced by a recompute attempt. Cleared on the next
    /// successful pass.
    private(set) var lastError: String?

    /// Wall-clock time of the last successful recompute. `nil` until the
    /// first pass completes.
    private(set) var lastRecomputedAt: Date?

    /// Late-bound reference to the application's `AppStateStore`. Set via
    /// `attach(store:)` from the view layer because the service is a
    /// singleton declared before the store is ready. Held weakly so the
    /// service never extends the store's lifetime.
    private(set) weak var store: AppStateStore?

    // MARK: Init

    init() {}

    // MARK: - Public API

    /// Bind the service to the running `AppStateStore`. Call from the
    /// surface that first needs the service (e.g. `ThreadingTopicListView`
    /// `.task`). Idempotent — repeated calls just refresh the reference.
    func attach(store: AppStateStore) {
        self.store = store
    }

    /// Re-derive every topic + mention from the user's transcript corpus.
    /// v1 is a stub: the real pipeline (RAG semantic search, noun-phrase
    /// extraction, contradiction detection) lands behind a later milestone.
    /// For now the call clears `lastError` and advances `lastRecomputedAt` so
    /// the UI hooks are exercised.
    func recompute(store: AppStateStore) async {
        guard !isRecomputing else { return }
        isRecomputing = true
        defer { isRecomputing = false }
        attach(store: store)
        // TODO: run noun-phrase extraction over the transcript corpus hosted
        // by `RAGService.shared.search`, cluster mentions per canonical slug,
        // score contradictions via prompt-driven verification, then
        // bulk-replace `state.threadingTopics` and `state.threadingMentions`.
        // Until that ships, `recompute` simply advances the timestamp so the
        // UI's "last refreshed" chrome is exercised — existing topics are
        // not disturbed so a mid-build seed keeps surfacing.
        lastError = nil
        lastRecomputedAt = Date()
    }

    /// Read-side accessor used by `ThreadingTopicView`. Returns an empty
    /// list when the service hasn't been attached yet (the timeline view
    /// will render its own empty state).
    func mentions(forTopic id: UUID) -> [ThreadingMention] {
        store?.threadingMentions(forTopic: id) ?? []
    }

    /// Topics with the most *unplayed* episode mentions, descending. Used by
    /// the Home "Threaded Today" pill to surface a thread the user can
    /// listen through in one sitting. A topic only qualifies if at least
    /// three distinct unplayed episodes mention it — anything thinner is
    /// just a recommendation, not a thread.
    ///
    /// Pass `subscriptionFilter` to scope the count to one category's
    /// shows: only mentions whose episode belongs to a subscription in
    /// the set are counted, and a topic with fewer than three qualifying
    /// mentions is dropped. `nil` keeps the original global behaviour the
    /// "All Categories" pseudo-category expects.
    ///
    /// Returns an empty list if no store is attached or no topic clears
    /// the threshold. The pill hides itself in that case.
    func topActiveTopics(
        limit: Int,
        subscriptionFilter: Set<UUID>? = nil
    ) -> [ActiveTopic] {
        guard let store else { return [] }

        // Build the eligible-episode id set up front. When a subscription
        // filter is supplied we narrow on `subscriptionID` first so the
        // "is this mention's episode unplayed" check downstream stays an
        // O(1) set membership.
        var eligibleEpisodeIDs: Set<UUID> = []
        for episode in store.state.episodes where !episode.played && !episode.isTriageArchived {
            if let allowed = subscriptionFilter,
               !allowed.contains(episode.podcastID) { continue }
            eligibleEpisodeIDs.insert(episode.id)
        }
        guard !eligibleEpisodeIDs.isEmpty else { return [] }

        // Build (topicID → unique-eligible-episodeIDs). Going through the
        // raw mention array directly avoids pulling
        // `threadingMentions(forTopic:)` for every topic, which would do an
        // n*m scan over the mention table.
        var unplayedByTopic: [UUID: Set<UUID>] = [:]
        for mention in store.state.threadingMentions where eligibleEpisodeIDs.contains(mention.episodeID) {
            unplayedByTopic[mention.topicID, default: []].insert(mention.episodeID)
        }

        let qualifying: [ActiveTopic] = unplayedByTopic.compactMap { topicID, episodes in
            guard episodes.count >= 3,
                  let topic = store.threadingTopic(id: topicID) else { return nil }
            return ActiveTopic(topic: topic, unplayedEpisodeCount: episodes.count)
        }
        return Array(
            qualifying
                .sorted { $0.unplayedEpisodeCount > $1.unplayedEpisodeCount }
                .prefix(max(1, limit))
        )
    }

    /// Topic + its unplayed-mention count. Surfaced by `topActiveTopics`
    /// so consumers can render `"3 episodes touch on $TOPIC"` without
    /// recomputing the count. Conforms to `Identifiable` (by the topic's
    /// id) so it can drive a SwiftUI `.sheet(item:)` directly.
    struct ActiveTopic: Sendable, Equatable, Identifiable {
        let topic: ThreadingTopic
        let unplayedEpisodeCount: Int
        var id: UUID { topic.id }
    }

    /// Idempotent get-or-create. If a topic with the canonicalised `slug`
    /// already exists, returns it untouched; otherwise inserts a fresh row
    /// and returns the stored instance. Used by deep-links from the wiki
    /// ("open thread for X") and by the in-episode agent. Returns the input
    /// topic verbatim if no store is attached — callers should `attach`
    /// before relying on persistence.
    @discardableResult
    func ensureTopic(
        slug: String,
        displayName: String
    ) -> ThreadingTopic {
        let normalized = WikiPage.normalize(slug: slug)
        let fresh = ThreadingTopic(
            slug: normalized,
            displayName: displayName,
            episodeMentionCount: 0,
            contradictionCount: 0,
            lastMentionedAt: nil
        )
        guard let store else { return fresh }
        if let existing = store.threadingTopic(slug: normalized) {
            return existing
        }
        return store.upsertThreadingTopic(fresh)
    }

    // MARK: - Mock seeding (debug-only)

    /// Inserts a small fixture set when the store has zero topics, so the UI
    /// surface is reviewable on a fresh install. No-op outside `DEBUG`
    /// builds and once any topic exists. The seed picks the three most
    /// recent episodes from the user's library so the timestamps it
    /// dispatches are valid.
    func seedMockIfEmpty(store: AppStateStore) {
        #if DEBUG
        guard store.state.threadingTopics.isEmpty else { return }
        let seedEpisodes = store.state.episodes
            .sorted { $0.pubDate > $1.pubDate }
            .prefix(3)
        guard seedEpisodes.count >= 2 else { return }
        let topic = ThreadingTopic(
            slug: "ketogenic-diet",
            displayName: "Ketogenic diet",
            definition: "A high-fat, very-low-carb dietary pattern that shifts the body into ketosis as its primary fuel mode.",
            episodeMentionCount: seedEpisodes.count,
            contradictionCount: 1,
            lastMentionedAt: seedEpisodes.first?.pubDate
        )
        let stored = store.upsertThreadingTopic(topic)
        let fixtureSnippets = [
            "the mental clarity has been incredible — best six weeks of my life",
            "long-term cardiac markers concern me, especially LDL",
            "properly formulated keto improves nearly every biomarker we test"
        ]
        var mentions: [ThreadingMention] = []
        for (idx, episode) in seedEpisodes.enumerated() {
            let snippet = fixtureSnippets[idx % fixtureSnippets.count]
            let isContradictory = idx == 1
            mentions.append(ThreadingMention(
                topicID: stored.id,
                episodeID: episode.id,
                startMS: 30_000 + idx * 60_000,
                endMS: 42_000 + idx * 60_000,
                snippet: snippet,
                confidence: 0.82 - Double(idx) * 0.07,
                isContradictory: isContradictory
            ))
        }
        store.replaceThreadingMentions(forTopic: stored.id, with: mentions)
        #endif
    }
}
