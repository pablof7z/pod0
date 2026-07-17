import Foundation

extension AppStateStore {
    func setRequestedTranscriptProvider(_ id: UUID, provider: STTProvider?) {
        updateEpisode(id) { $0.requestedTranscriptProvider = provider }
    }

    func resetOrphanedTranscriptState(_ id: UUID) {
        // JobStore lease recovery owns interrupted work. Episode stores only
        // stable artifact availability, so there is nothing to reset here.
    }

    private func updateEpisode(_ id: UUID, mutation: (inout Episode) -> Void) {
        guard let index = state.episodes.firstIndex(where: { $0.id == id }) else { return }
        var episodes = state.episodes
        mutation(&episodes[index])
        performMutationBatch {
            mutateState { $0.episodes = episodes }
            invalidateEpisodeProjections()
        }
    }
}
