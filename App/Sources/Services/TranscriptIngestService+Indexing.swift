import Foundation

extension TranscriptIngestService {
    /// Runs only the vector-index outcome for an already persisted transcript.
    /// Failures escape to `WorkCoordinator`, which owns durable backoff.
    func indexTranscript(
        episodeID: UUID,
        generation: String
    ) async throws -> VectorArtifactReceipt {
        guard let appStore = rag.appStore,
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
        let chunks = chunkBuilder.build(from: ChunkableTranscript(
            transcript: transcript,
            podcastID: episode.podcastID
        ))
        let receipt = try await rag.index.stageArtifact(
            chunks: chunks,
            episodeID: episode.id,
            generation: generation,
            artifactKind: VectorIndex.semanticArtifactKind
        )
        Self.logger.info(
            "indexed \(chunks.count, privacy: .public) transcript chunks for \(episode.id, privacy: .public)"
        )
        if let selectedData = store.verifiedData(episodeID: episodeID) {
            appStore.sharedLibrary?.scheduleTranscriptEvidenceRebuild(
                transcript: transcript,
                podcastID: episode.podcastID,
                selectedData: selectedData
            )
        }
        return receipt
    }
}
