import Foundation

// MARK: - AppStateStore + Triage
//
// Display-side interaction surface for AI Inbox triage. The Rust kernel
// owns triage (M5): it selects candidates, runs the classifier, and projects
// per-episode decisions onto `Episode.triageDecision` every snapshot tick.
// Swift no longer runs triage orchestration — the only local mutation left
// here is `clearTriageDecision`, used when the user rescues an archived
// episode by playing it, which optimistically clears the local decision and
// reports the clear to the kernel so the projection doesn't resurrect it.

extension AppStateStore {

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
        // M4 / D7: clear the decision in Rust via the "none" sentinel so the
        // next projection pass doesn't resurrect the stale decision.
        kernelSetEpisodeTriage([
            KernelTriagePatch(episodeID: episodeID, decision: "none", isHero: false, rationale: nil)
        ])
    }
}
