import Foundation

// MARK: - AppStateStore + Triage
//
// Mutation surface for the AI Inbox triage pass. The triage pass produces
// one decision per untriaged episode; callers funnel the full batch through
// `applyTriageDecisions(_:)` to land them in a single mutation batch so
// SwiftUI surfaces re-render once per pass instead of once per episode.

extension AppStateStore {

    /// One row produced by the triage pass — an episode id paired with the
    /// agent's decision, the one-line rationale shown on the Home Inbox
    /// card (for `.inbox`), and an optional hero flag promoting the row
    /// to the single hero pick of the pass.
    struct TriagePatch: Sendable, Hashable {
        let episodeID: UUID
        let decision: TriageDecision
        let rationale: String?
        let isHero: Bool

        init(episodeID: UUID, decision: TriageDecision, rationale: String?, isHero: Bool = false) {
            self.episodeID = episodeID
            self.decision = decision
            self.rationale = rationale
            self.isHero = isHero
        }
    }

    /// Apply a batch of triage decisions to `state.episodes`. Single
    /// mutation batch + projection invalidation; missing episode IDs
    /// (e.g. removed between the LLM call and the apply) are silently
    /// skipped. Safe to call with an empty array.
    ///
    /// **Invariant:** `.inbox` patches must carry a non-blank rationale —
    /// patches that violate this are dropped at the boundary so a future
    /// caller can't land empty Inbox cards on Home. `.archived` patches
    /// always have their rationale nil'd out.
    func applyTriageDecisions(_ patches: [TriagePatch]) {
        guard !patches.isEmpty else { return }
        let valid = patches.filter { patch in
            guard patch.decision == .inbox else { return true }
            guard let rationale = patch.rationale,
                  !rationale.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            else { return false }
            return true
        }
        guard !valid.isEmpty else { return }
        let byID = Dictionary(uniqueKeysWithValues: valid.map { ($0.episodeID, $0) })
        var episodes = state.episodes
        var changed = false
        // Hero only applies to inbox patches. Compute up-front whether
        // *any* incoming inbox patch claims hero — if so, every other
        // episode loses its hero flag in this pass so we never end up
        // with stale heroes alongside a fresh one.
        let incomingHero = valid.contains { $0.decision == .inbox && $0.isHero }
        for idx in episodes.indices {
            guard let patch = byID[episodes[idx].id] else { continue }
            let nextHero = patch.decision == .inbox && patch.isHero
            // Skip if the same decision is already recorded — avoids
            // writing a no-op patch that would still bust the projection
            // cache and trigger a full re-encode of the episodes array.
            if episodes[idx].triageDecision == patch.decision,
               episodes[idx].triageRationale == patch.rationale,
               episodes[idx].triageIsHero == nextHero {
                continue
            }
            episodes[idx].triageDecision = patch.decision
            episodes[idx].triageRationale = patch.decision == .inbox ? patch.rationale : nil
            episodes[idx].triageIsHero = nextHero
            changed = true
        }
        // Demote any prior hero NOT in this batch when the new batch
        // crowned a fresh hero — keeps the "at most one hero" invariant
        // across passes.
        if incomingHero {
            for idx in episodes.indices where episodes[idx].triageIsHero && byID[episodes[idx].id] == nil {
                episodes[idx].triageIsHero = false
                changed = true
            }
        }
        guard changed else { return }
        performMutationBatch {
            state.episodes = episodes
            invalidateEpisodeProjections()
        }
    }

    /// Clear a single episode's triage state. Used when the user
    /// manually rescues an archived episode (future surface) or when a
    /// re-triage pass wants to overwrite a previous run.
    func clearTriageDecision(_ episodeID: UUID) {
        guard let idx = state.episodes.firstIndex(where: { $0.id == episodeID }) else { return }
        guard state.episodes[idx].triageDecision != nil else { return }
        var episodes = state.episodes
        episodes[idx].triageDecision = nil
        episodes[idx].triageRationale = nil
        episodes[idx].triageIsHero = false
        performMutationBatch {
            state.episodes = episodes
            invalidateEpisodeProjections()
        }
    }
}
