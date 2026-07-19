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
              let sharedLibrary = appStore.sharedLibrary,
              let transcript = sharedLibrary.authoritativeTranscriptReader.load(
                episodeID: episodeID
              ),
              let summary = try sharedLibrary.authoritativeTranscriptReader.summary(
                episodeID: episodeID
              ) else {
            throw JobFailure(
                classification: .missingDependency,
                message: "Transcript is not available for indexing."
            )
        }
        let receipt = try await sharedLibrary.rebuildTranscriptEvidence(
            transcript: transcript,
            summary: summary,
            inputVersion: generation
        )
        Self.logger.info(
            "indexed \(receipt.spanCount, privacy: .public) shared evidence spans for \(episode.id, privacy: .public)"
        )
        return receipt
    }
}
