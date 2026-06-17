import Foundation

// MARK: - AppStateStore + Triage
//
// Display-side interaction surface for AI Inbox triage. The Rust kernel owns
// triage (M5): it selects candidates, runs the classifier, projects per-episode
// decisions, and clears a decision when user playback intent rescues an item.

extension AppStateStore {

    /// Clear a single episode's triage state through the kernel. Kept as a
    /// thin action wrapper for explicit future UI affordances; playback rescue
    /// is handled by Rust `podcast.player` load/play.
    func clearTriageDecision(_ episodeID: UUID) {
        kernelSetEpisodeTriage([
            KernelTriagePatch(episodeID: episodeID, decision: "none", isHero: false, rationale: nil)
        ])
    }
}
