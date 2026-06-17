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
        // Every clip funnels through here (composer + auto-snip), so it's the
        // single seam to record clip creation in the episode's Diagnostics log
        // — what was clipped, from where, and how.
        kernelRecordEpisodeEvent(
            episodeID: clip.episodeID,
            kind: "clip.created",
            severity: "info",
            summary: "Clip created · \(Self.clipSpanLabel(clip))",
            details: [
                ("Span", Self.clipSpanLabel(clip)),
                ("Source", clip.source.rawValue),
            ]
        )
        // Wiring contract per `identity-05-synthesis.md` §5.3: every clip
        // source signs and publishes (kind 9802 / NIP-84) except `.agent`,
        // which stays local. Fire-and-forget so a relay outage never blocks
        // the user's local capture.
        if clip.source != .agent {
            let ep  = episode(id: clip.episodeID)
            let pod = ep.flatMap { podcast(id: $0.podcastID) }
            Task { try? await identity.publishUserClip(clip, episode: ep, podcast: pod) }
        }
    }

    /// `M:SS–M:SS` label for a clip's span, used in Diagnostics event summaries.
    nonisolated static func clipSpanLabel(_ clip: Clip) -> String {
        func fmt(_ ms: Int) -> String {
            let total = max(0, ms) / 1000
            return String(format: "%d:%02d", total / 60, total % 60)
        }
        return "\(fmt(clip.startMs))–\(fmt(clip.endMs))"
    }

    /// Convenience: build + persist in one call. Used by the in-app clip
    /// composer and agent-generated clip pathways. The transcript text may be
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

    func deleteClip(id: UUID) {
        guard let idx = state.clips.firstIndex(where: { $0.id == id }) else { return }
        state.clips.remove(at: idx)
    }

    // MARK: - Kernel projection (read-side inversion)

    /// Project the kernel's `ClipSummary` rows (reactive snapshot) into
    /// `state.clips`. This is the READ-side of the clips arc: kernel-owned
    /// clips — AutoSnip captures (which now dispatch `podcast.clip auto_snip`)
    /// and clips persisted across restart (SLICE 1) — surface in the Clippings
    /// UI through this seam. Without it, an AutoSnip dispatch creates+persists
    /// the clip in the kernel but the iOS UI (which reads `state.clips`) never
    /// sees it. Android already does the equivalent (`snapshot.clips` ← the
    /// `podcast.misc` frame); iOS was never wired.
    ///
    /// MERGE semantics (not blind SET): the kernel `ClipSummary` is lossy
    /// relative to the domain `Clip` — it carries no `transcriptText`,
    /// `speakerID`, or `source`, and no `subscriptionID`. The in-app composer
    /// (`ClipComposerSheet` / `ClippingsView`) builds rich local clips that are
    /// NOT dispatched to the kernel and are persisted Swift-side via `AppState`.
    /// A blind `state.clips = kernel.map(...)` would strip those clips' rich
    /// fields (or drop composer clips entirely). So the merge is keyed by clip
    /// id and discriminated by ownership (`kernelClipIDs`):
    ///   • Every clip the kernel currently reports is surfaced. If the id is
    ///     also present locally with richer data, the local version wins (the
    ///     AutoSnip-then-rename case stays correct once create/delete route
    ///     through the kernel in a later slice); otherwise it's mapped in via
    ///     `Clip(from:)` — the AutoSnip / restart-persisted surface.
    ///   • A clip the kernel PREVIOUSLY reported (in `kernelClipIDs`) but no
    ///     longer does is DROPPED — that's a kernel-side delete propagating.
    ///   • Composer-authored local clips (never kernel-owned) are preserved so
    ///     their rich data and Swift-side persistence survive.
    /// `kernelClipIDs` is the discriminator that lets the projection tell a
    /// kernel-owned clip already sitting in `state.clips` apart from a local
    /// one on the next tick — without it, a projected kernel clip would look
    /// local and could never be removed on kernel-side delete.
    ///
    /// `subscriptionID` is resolved from `episodeToPodcast` (built once per
    /// projection from the library), falling back to the Unknown sentinel.
    func projectKernelClips(
        _ summaries: [ClipSummary],
        episodeToPodcast: [UUID: UUID]
    ) {
        let localByID = Dictionary(
            state.clips.map { ($0.id, $0) },
            uniquingKeysWith: { first, _ in first })
        let priorKernelIDs = kernelClipIDs

        var merged: [Clip] = []
        merged.reserveCapacity(max(state.clips.count, summaries.count))
        var newKernelIDs = Set<UUID>()
        var seen = Set<UUID>()

        // Kernel-reported clips: existence is authoritative. Prefer the local
        // (richer) version when the same id is also present locally.
        for summary in summaries {
            guard let id = UUID(uuidString: summary.id) else { continue }
            guard seen.insert(id).inserted else { continue }
            newKernelIDs.insert(id)
            if let local = localByID[id] {
                merged.append(local)
            } else {
                let episodeID = UUID(uuidString: summary.episodeId)
                let subscriptionID = episodeID.flatMap { episodeToPodcast[$0] }
                    ?? Podcast.unknownID
                merged.append(Clip(from: summary, subscriptionID: subscriptionID))
            }
        }
        // Local clips not in the current kernel set: keep them UNLESS they were
        // previously kernel-owned (now absent ⇒ deleted in the kernel ⇒ drop).
        for clip in state.clips where !seen.contains(clip.id) {
            if priorKernelIDs.contains(clip.id) { continue }
            merged.append(clip)
        }

        // Sort newest-first to match `allClips()` / `ClippingsView` ordering so
        // the stored array is already in a stable, expected order.
        merged.sort { $0.createdAt > $1.createdAt }

        kernelClipIDs = newKernelIDs
        if merged != state.clips {
            state.clips = merged
        }
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
