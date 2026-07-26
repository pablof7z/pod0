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
    func addClip(_ clip: Clip) async -> Clip? {
        do {
            guard let sharedLibrary else { throw SharedLibraryError.unavailable }
            let saved = try await sharedLibrary.createClip(clip)
            recordProductSignal(.once(
                name: .clipCreated,
                subjectID: saved.id,
                outcome: .created
            ))
            return saved
        } catch {
            Self.clipsLogger.error(
                "Shared clip creation failed: \(error.localizedDescription, privacy: .public)"
            )
            return nil
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
    ) async -> Clip? {
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
        return await addClip(clip)
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
    ) async -> Bool {
        guard var clip = sharedLibrary?.clip(id: id) else { return false }
        clip.startMs = startMs
        clip.endMs = endMs
        clip.transcriptText = transcriptText
        clip.speakerID = speakerID?.uuidString
        do {
            guard let sharedLibrary else { throw SharedLibraryError.unavailable }
            try await sharedLibrary.updateClip(clip)
            return true
        } catch {
            Self.clipsLogger.error(
                "Shared clip update failed: \(error.localizedDescription, privacy: .public)"
            )
            return false
        }
    }

    @discardableResult
    func deleteClip(id: UUID) async -> Bool {
        do {
            guard let clip = sharedLibrary?.clip(id: id),
                  let sharedLibrary
            else { throw SharedLibraryError.notFound }
            try await sharedLibrary.setClipDeleted(clip, deleted: true)
            return true
        } catch {
            Self.clipsLogger.error(
                "Shared clip deletion failed: \(error.localizedDescription, privacy: .public)"
            )
            return false
        }
    }

    func clip(id: UUID) -> Clip? {
        state.clips.first { $0.id == id && !$0.deleted }
    }

    /// All clips, newest first. Used by the Clips screen.
    func allClips() -> [Clip] {
        state.clips.filter { !$0.deleted }
    }

    /// Clips for a single episode, newest first. Used by the episode detail
    /// surface and the global clips list.
    func clips(forEpisode id: UUID) -> [Clip] {
        state.clips.filter { $0.episodeID == id && !$0.deleted }
    }

    @discardableResult
    func clearAllClips() async -> Bool {
        do {
            guard let sharedLibrary else { throw SharedLibraryError.unavailable }
            try await sharedLibrary.clearClips()
            return true
        } catch {
            Self.clipsLogger.error(
                "Shared clip clear failed: \(error.localizedDescription, privacy: .public)"
            )
            return false
        }
    }
}
