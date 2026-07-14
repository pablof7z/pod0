import Foundation
import os.log

// MARK: - Clips

/// CRUD surface for user-authored transcript excerpts. Mirrors the pattern
/// used by `+Notes` and `+Memories` so all clip mutations route through one
/// place and the `state.didSet` observer in `AppStateStore` picks them up
/// for persistence + Spotlight + widget refresh.
///
/// Auto-snip and the in-app composer both land here so a clip captured from
/// the lock-screen and a clip composed from a transcript share the same
/// storage and the same observer chain.
extension AppStateStore {

    nonisolated private static let clipsLogger = Logger.app("AppStateStore+Clips")

    func addClip(_ clip: Clip) {
        state.clips.append(clip)
        // Wiring contract per `identity-05-synthesis.md` §5.3: every clip
        // source signs and publishes (kind 9802 / NIP-84) except `.agent`,
        // which stays local. Fire-and-forget so a relay outage never blocks
        // the user's local capture.
        if clip.source != .agent {
            let ep  = episode(id: clip.episodeID)
            let pod = ep.flatMap { podcast(id: $0.podcastID) }
            Task { try? await UserIdentityStore.shared.publishUserClip(clip, episode: ep, podcast: pod) }
        }
    }

    /// Convenience: build + persist in one call. Used by `AutoSnipController`
    /// (auto / headphone / lock-screen pathways). The transcript window may be
    /// `nil` when the episode hasn't been ingested yet — we collapse to an
    /// empty string so the rest of the share stack stays string-typed.
    @discardableResult
    func addClip(
        episodeID: UUID,
        subscriptionID: UUID,
        startMs: Int,
        endMs: Int,
        transcriptText: String? = nil,
        speakerID: UUID? = nil,
        source: Clip.Source = .auto,
        caption: String? = nil
    ) -> Clip {
        let clip = Clip(
            episodeID: episodeID,
            subscriptionID: subscriptionID,
            startMs: startMs,
            endMs: endMs,
            caption: caption,
            speakerID: speakerID?.uuidString,
            transcriptText: transcriptText ?? "",
            source: source
        )
        // Route through the primary `addClip(_:)` so the publish wiring
        // fires uniformly for every entry-point (composer + auto-snip).
        addClip(clip)
        return clip
    }

    /// In-place rewrite for the optimistic-then-refine flow used by
    /// `AutoSnipController`: the mechanical clip lands first (instant haptic +
    /// toast), then a background LLM call refines the boundaries and calls
    /// this to overwrite the span and frozen transcript. We deliberately do
    /// NOT re-publish NIP-84 here — the initial publish in `addClip(_:)` is
    /// the user-visible event; refinement is local polish.
    func updateClipBoundaries(
        id: UUID,
        startMs: Int,
        endMs: Int,
        transcriptText: String,
        speakerID: UUID?
    ) {
        guard let idx = state.clips.firstIndex(where: { $0.id == id }) else { return }
        var clip = state.clips[idx]
        clip.startMs = startMs
        clip.endMs = endMs
        clip.transcriptText = transcriptText
        clip.speakerID = speakerID?.uuidString
        state.clips[idx] = clip
    }

    func deleteClip(id: UUID) {
        guard let idx = state.clips.firstIndex(where: { $0.id == id }) else { return }
        state.clips.remove(at: idx)
    }

    func clip(id: UUID) -> Clip? {
        state.clips.first(where: { $0.id == id })
    }

    /// All clips, newest first. Used by the Clippings tab.
    func allClips() -> [Clip] {
        state.clips.sorted { $0.createdAt > $1.createdAt }
    }

    /// Clips for a single episode, newest first. Used by the episode detail
    /// surface and the global clips list.
    func clips(forEpisode id: UUID) -> [Clip] {
        state.clips
            .filter { $0.episodeID == id }
            .sorted { $0.createdAt > $1.createdAt }
    }
}
