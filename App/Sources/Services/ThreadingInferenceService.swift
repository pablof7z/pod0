import Foundation
import Observation

// MARK: - Threading projection service

/// Thin UI-facing handle for the Rust-owned cross-episode threading projection.
///
/// Rust derives topics and mentions from kernel library, transcript, and
/// categorization facts. Swift owns only refresh timing and native rendering
/// affordances such as progress chrome.
@MainActor
@Observable
final class ThreadingInferenceService {

    static let shared = ThreadingInferenceService()

    private(set) var isRecomputing: Bool = false
    private(set) var lastError: String?
    private(set) var lastRecomputedAt: Date?
    private(set) weak var store: AppStateStore?

    init() {}

    func attach(store: AppStateStore) {
        self.store = store
        store.refreshThreadingProjection()
    }

    func recompute(store: AppStateStore) async {
        guard !isRecomputing else { return }
        isRecomputing = true
        defer { isRecomputing = false }
        attach(store: store)
        lastError = nil
        lastRecomputedAt = Date()
    }

    func mentions(forTopic id: UUID) -> [ThreadingMention] {
        store?.threadingMentions(forTopic: id) ?? []
    }

    func topActiveTopics(
        limit: Int,
        subscriptionFilter: Set<UUID>? = nil
    ) -> [ActiveTopic] {
        store?.activeThreadingTopics(limit: limit, subscriptionFilter: subscriptionFilter).map {
            ActiveTopic(
                topic: $0.topic,
                unplayedEpisodeCount: $0.unplayedEpisodeCount,
                mentionIDs: $0.mentionIDs
            )
        } ?? []
    }

    struct ActiveTopic: Sendable, Equatable, Identifiable {
        let topic: ThreadingTopic
        let unplayedEpisodeCount: Int
        let mentionIDs: [UUID]
        var id: UUID { topic.id }
    }
}
