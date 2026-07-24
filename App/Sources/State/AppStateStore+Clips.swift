import Foundation
import os.log

// MARK: - Clips

/// Native adapter for Rust-owned user-authored transcript excerpts.
///
/// Auto-snip and the in-app composer both land here so a clip captured from
/// the lock-screen and a clip composed from a transcript share the same
/// storage and the same observer chain.
extension AppStateStore {

    nonisolated private static let clipsLogger = Logger.app("AppStateStore+Clips")

    @discardableResult
    func addClip(_ clip: Clip) -> Bool {
        do {
            guard let sharedLibrary else { throw SharedLibraryError.unavailable }
            let saved = try sharedLibrary.createClip(clip)
            recordProductSignal(.once(
                name: .clipCreated,
                subjectID: saved.id,
                outcome: .created
            ))
            return true
        } catch {
            Self.clipsLogger.error(
                "Shared clip creation failed: \(error.localizedDescription, privacy: .public)"
            )
            return false
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
    ) -> Clip? {
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
        guard addClip(clip) else { return nil }
        return sharedLibrary?.clip(id: clip.id)
    }

    /// In-place rewrite for the optimistic-then-refine flow used by
    /// `AutoSnipController`: the mechanical clip lands first (instant haptic +
    /// toast), then a background LLM call refines the boundaries and calls
    /// this to overwrite the span and frozen transcript.
    @discardableResult
    func updateClipBoundaries(
        id: UUID,
        startMs: Int,
        endMs: Int,
        transcriptText: String,
        speakerID: UUID?
    ) -> Bool {
        guard var clip = sharedLibrary?.clip(id: id) else { return false }
        clip.startMs = startMs
        clip.endMs = endMs
        clip.transcriptText = transcriptText
        clip.speakerID = speakerID?.uuidString
        do {
            guard let sharedLibrary else { throw SharedLibraryError.unavailable }
            try sharedLibrary.updateClip(clip)
            return true
        } catch {
            Self.clipsLogger.error(
                "Shared clip update failed: \(error.localizedDescription, privacy: .public)"
            )
            return false
        }
    }

    @discardableResult
    func deleteClip(id: UUID) -> Bool {
        do {
            guard let clip = sharedLibrary?.clip(id: id),
                  let sharedLibrary
            else { throw SharedLibraryError.notFound }
            try sharedLibrary.setClipDeleted(clip, deleted: true)
            return true
        } catch {
            Self.clipsLogger.error(
                "Shared clip deletion failed: \(error.localizedDescription, privacy: .public)"
            )
            return false
        }
    }

    func clip(id: UUID) -> Clip? {
        sharedLibrary?.clip(id: id)
    }

    /// All clips, newest first. Used by the Clips screen.
    func allClips() -> [Clip] {
        sharedLibrary?.allClips() ?? []
    }

    /// Clips for a single episode, newest first. Used by the episode detail
    /// surface and the global clips list.
    func clips(forEpisode id: UUID) -> [Clip] {
        sharedLibrary?.clips(forEpisode: id) ?? []
    }

    @discardableResult
    func clearAllClips() -> Bool {
        do {
            guard let sharedLibrary else { throw SharedLibraryError.unavailable }
            try sharedLibrary.clearClips()
            return true
        } catch {
            Self.clipsLogger.error(
                "Shared clip clear failed: \(error.localizedDescription, privacy: .public)"
            )
            return false
        }
    }
}
