import Foundation

// MARK: - Per-category settings
//
// Legacy category settings helpers plus renderer-facing helpers. Rust owns
// active transcription policy; `state.categorySettings` is read only by the
// one-shot migration that mirrors old disabled category transcription into
// per-podcast kernel policy.

extension AppStateStore {

    // NOTE: Auto-download evaluation ("which episodes should download right
    // now, given the policy + Wi-Fi state") is owned entirely by the Rust
    // kernel (M2): `PodcastAction::SetAutoDownload { enabled, wifi_only }`
    // records the policy and `episodes_to_auto_download` / the Wi-Fi-gated
    // batch decide what actually downloads. iOS no longer resolves a policy
    // to drive downloads; it only dispatches the user's choice through
    // `kernelSetAutoDownload` and reads the current setting back from the
    // kernel snapshot for display. The former `effectiveAutoDownload(forPodcast:)`
    // resolver lived here and was already dead (no callers) once the kernel
    // took over the decision; it has been removed rather than left as a trap.

    /// True when transcription should run for episodes of `podcastID`.
    /// Reads the kernel-owned per-podcast override
    /// (`PodcastSummary.transcriptionEnabled`), which survives library
    /// rebuilds. If the kernel snapshot has not arrived yet, Swift returns the
    /// permissive default instead of re-deriving category policy locally.
    func effectiveTranscriptionEnabled(forPodcast podcastID: UUID) -> Bool {
        if let summary = kernel?.library.first(where: { $0.id == podcastID.uuidString.lowercased() }) {
            return summary.transcriptionEnabled
        }
        return true
    }
}
