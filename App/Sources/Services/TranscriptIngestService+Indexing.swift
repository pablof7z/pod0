import Foundation

extension TranscriptIngestService {
    /// Runs only the vector-index outcome for an already persisted transcript.
    /// Failures escape to `WorkCoordinator`, which owns durable backoff.
    func indexTranscript(
        episodeID: UUID,
        generation: String
    ) async throws -> SharedEvidenceReceipt {
        guard let appStore,
              let episode = appStore.episode(id: episodeID) else {
            throw JobFailure(classification: .invalidInput, message: "Episode no longer exists")
        }
        guard case .ready = episode.transcriptState,
              let transcript = store.load(episodeID: episodeID) else {
            throw JobFailure(
                classification: .missingDependency,
                message: "Transcript is not available for indexing."
            )
        }
        guard let selectedData = store.verifiedData(episodeID: episodeID),
              let sharedLibrary = appStore.sharedLibrary else {
            throw JobFailure(
                classification: .missingDependency,
                message: "Shared evidence storage is unavailable."
            )
        }
        let receipt = try await sharedLibrary.rebuildTranscriptEvidence(
            transcript: transcript,
            podcastID: episode.podcastID,
            selectedData: selectedData,
            inputVersion: generation
        )
        Self.logger.info(
            "indexed \(receipt.spanCount, privacy: .public) shared evidence spans for \(episode.id, privacy: .public)"
        )
        return receipt
    }
}
