import Foundation

extension AppStateStore {
    func applySharedLibrary(_ projection: SharedLibrarySnapshot) {
        let existingEpisodes = Dictionary(uniqueKeysWithValues: state.episodes.map { ($0.id, $0) })
        let projectedPodcasts = projection.podcasts.map(\.swiftValue)
        let projectedEpisodes = projection.episodes.compactMap { record in
            record.episodeId.uuid.flatMap {
                record.swiftValue(
                    preserving: existingEpisodes[$0],
                    chapters: projection.chaptersByEpisodeID[$0],
                    downloadState: sharedLibrary?.downloadState(for: record.download)
                        ?? .notDownloaded
                )
            }
        }
        let newlyReadyTranscriptIDs = projectedEpisodes.compactMap { episode -> UUID? in
            guard Self.isTranscriptReady(episode.transcriptState),
                  !Self.isTranscriptReady(existingEpisodes[episode.id]?.transcriptState)
            else { return nil }
            return episode.id
        }
        performMutationBatch {
            mutateProjectionState {
                $0.podcasts = projectedPodcasts
                $0.subscriptions = projection.subscriptions.map(\.swiftValue)
                $0.episodes = projectedEpisodes
            }
            invalidateEpisodeProjections()
        }
        for episodeID in newlyReadyTranscriptIDs {
            recordProductSignal(.once(
                name: .transcriptReady,
                subjectID: episodeID,
                outcome: .ready
            ))
        }
    }

    /// Replaces the native read model from a bounded Rust projection. This is
    /// the only production assignment to `AppState.notes`; Persistence strips
    /// it from metadata once shared note authority is active.
    func applySharedNotes(_ projection: SharedNoteSnapshot) {
        mutateProjectionState { $0.notes = projection.notes }
    }

    /// The sole production assignment to the replaceable native memory read model.
    func applySharedMemories(_ projection: SharedMemorySnapshot) {
        mutateProjectionState {
            $0.agentMemories = projection.memories
            $0.compiledMemory = projection.compiled
        }
    }

    /// The sole production assignment to the replaceable native clip read model.
    func applySharedClips(_ projection: SharedClipSnapshot) {
        mutateProjectionState { $0.clips = projection.clips }
    }

    /// Applies one bounded Rust chapter projection to the replaceable native
    /// read model. Swift never persists or independently mutates these values.
    func applySharedChapter(_ projection: SharedChapterSnapshot) {
        guard let episodeID = projection.summary.episodeId.uuid,
              let index = state.episodes.firstIndex(where: { $0.id == episodeID })
        else { return }
        mutateProjectionState {
            $0.episodes[index].chapters = projection.chapters.isEmpty
                ? nil
                : projection.chapters
            $0.episodes[index].adSegments = projection.adSegments
        }
    }

    func clearSharedChapter(episodeID: UUID) {
        guard let index = state.episodes.firstIndex(where: { $0.id == episodeID }) else {
            return
        }
        guard state.episodes[index].chapters != nil
                || state.episodes[index].adSegments != nil else { return }
        mutateProjectionState {
            $0.episodes[index].chapters = nil
            $0.episodes[index].adSegments = nil
        }
    }

    private static func isTranscriptReady(_ state: TranscriptState?) -> Bool {
        guard case .ready = state else { return false }
        return true
    }
}
