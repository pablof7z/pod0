import Foundation

// MARK: - Clips

/// Read/delete surface for Rust-owned transcript excerpts. Creation routes
/// through `podcast.clip` actions; this file only maps the kernel projection
/// into the Swift `Clip` DTO used by rendering and share/export UI.
extension AppStateStore {

    /// `M:SS–M:SS` label for a clip's span, used in Diagnostics event summaries.
    nonisolated static func clipSpanLabel(_ clip: Clip) -> String {
        func fmt(_ ms: Int) -> String {
            let total = max(0, ms) / 1000
            return String(format: "%d:%02d", total / 60, total % 60)
        }
        return "\(fmt(clip.startMs))–\(fmt(clip.endMs))"
    }

    func deleteClip(id: UUID) {
        kernelDeleteClip(id: id)
    }

    func clip(id: UUID) -> Clip? {
        allClips().first(where: { $0.id == id })
    }

    /// All clips, newest first. Used by the Clippings tab.
    func allClips() -> [Clip] {
        kernelProjectedClips()
    }

    /// Clips for a single episode, newest first. Used by the episode detail
    /// surface and the global clips list.
    func clips(forEpisode id: UUID) -> [Clip] {
        kernelProjectedClips().filter { $0.episodeID == id }
    }

    private func kernelProjectedClips() -> [Clip] {
        guard let summaries = kernel?.podcastSnapshot?.clips, !summaries.isEmpty else {
            return []
        }
        return summaries.compactMap { summary in
            guard let clipID = UUID(uuidString: summary.id),
                  let episodeID = UUID(uuidString: summary.episodeId) else { return nil }
            let episode = episode(id: episodeID)
            let podcastID = episode?.podcastID
                ?? UUID(uuidString: "00000000-0000-0000-0000-000000000000")!
            return Clip(
                id: clipID,
                episodeID: episodeID,
                subscriptionID: podcastID,
                startMs: Int((summary.startSecs * 1000).rounded()),
                endMs: Int((summary.endSecs * 1000).rounded()),
                createdAt: Date(timeIntervalSince1970: TimeInterval(summary.createdAt)),
                caption: summary.title,
                speakerID: summary.speaker,
                transcriptText: summary.transcriptText,
                source: Clip.Source(rawValue: summary.source) ?? .auto
            )
        }
    }
}
