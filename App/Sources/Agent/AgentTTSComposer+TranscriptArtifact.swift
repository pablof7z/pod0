import Foundation

extension AgentTTSComposer {
    /// Commits the generated transcript's provenance before exposing its
    /// stable episode projection. This keeps synthetic episodes on the same
    /// evidence path as workflow-produced transcripts.
    @MainActor
    func commitGeneratedTranscript(
        _ transcript: Transcript,
        for episode: Episode
    ) throws {
        guard let store else { throw AgentTTSError.storeUnavailable }
        try TranscriptStore.shared.save(transcript)
        let url = TranscriptStore.shared.fileURL(for: episode.id)
        guard let data = TranscriptStore.shared.verifiedData(
            at: url,
            episodeID: episode.id
        ) else {
            throw JobFailure(
                classification: .unexpected,
                message: "Generated transcript failed verification."
            )
        }
        let inputVersion = DesiredStatePlanner.audioVersion(episode)
        let hash = ArtifactRepository.hash(data)
        try ArtifactRepository(
            fileURL: store.persistence.episodeStore.fileURL
        ).adopt(ArtifactRecord(
            kind: .transcript,
            subjectID: episode.id,
            inputVersion: inputVersion,
            outputVersion: hash,
            contentHash: hash,
            location: url.path,
            origin: TranscriptState.Source.other.rawValue,
            schemaVersion: 1,
            integrity: .available,
            verifiedAt: Date()
        ))
        let result = store.applyTranscriptEvent(.artifactCommitted(.init(
            inputVersion: inputVersion,
            contentHash: hash,
            fileURL: url,
            source: .other
        )), episodeID: episode.id)
        guard result == .applied || result == .noOp else {
            throw JobFailure(
                classification: .unexpected,
                message: "Generated transcript projection was rejected: \(result)"
            )
        }
    }
}
