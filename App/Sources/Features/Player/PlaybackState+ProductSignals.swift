import Foundation

extension PlaybackState {
    func recordPlaybackSignal(
        name: ProductSignalName,
        outcome: ProductSignalOutcome,
        errorClass: ProductFailureCode? = nil
    ) {
        let observation = ProductSignalObservation(
            name: name,
            outcome: outcome,
            errorClass: errorClass
        )
        Task { await productSignals.record(observation) }
    }

    func recordResumeAttempt(expectedPosition: TimeInterval) {
        let succeeded = abs(engine.currentTime - expectedPosition) <= 1
        recordPlaybackSignal(
            name: .resumeAttempt,
            outcome: succeeded ? .succeeded : .failed,
            errorClass: succeeded ? nil : .unexpected
        )
    }

    func recordMeaningfulListening(episodeID: UUID, domainRevision: UInt64) {
        guard recordedMeaningfulEpisodeIDs.insert(episodeID).inserted else { return }
        let observation = ProductSignalObservation.once(
            name: .meaningfulListening,
            subjectID: episodeID,
            outcome: .succeeded,
            domainRevision: domainRevision
        )
        Task { await productSignals.record(observation) }
    }
}
