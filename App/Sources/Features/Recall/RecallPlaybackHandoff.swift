import Foundation

enum RecallPlaybackHandoff {
    @MainActor
    @discardableResult
    static func open(
        _ evidence: RecallEvidence,
        store: AppStateStore,
        playback: PlaybackState
    ) -> Bool {
        guard let episode = store.episode(id: evidence.episodeID) else { return false }
        playback.setEpisode(episode)
        playback.seek(to: Double(evidence.startMilliseconds) / 1_000)
        if !playback.isPlaying { playback.play() }
        RecallQualityLogger.citationTapped()
        store.recordProductSignal(.init(name: .recallCitationOpened, outcome: .opened))
        return true
    }
}
