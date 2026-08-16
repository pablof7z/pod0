import Foundation
import Pod0Core

enum RecallPlaybackHandoff {
    @MainActor
    @discardableResult
    static func open(
        _ evidence: RecallEvidenceProjection,
        responseID: UUID,
        store: AppStateStore,
        playback: PlaybackState
    ) -> Bool {
        guard let episodeID = evidence.episodeId.uuid,
              let episode = store.episode(id: episodeID) else { return false }
        playback.setEpisode(episode)
        playback.seek(to: Double(evidence.startMilliseconds) / 1_000)
        if !playback.isPlaying { playback.play() }
        RecallQualityLogger.citationTapped()
        store.recordProductSignal(.once(
            name: .recallCitationOpened,
            subjectID: responseID,
            outcome: .opened
        ))
        return true
    }
}
