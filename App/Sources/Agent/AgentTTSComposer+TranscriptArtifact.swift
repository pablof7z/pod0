import Foundation

extension AgentTTSComposer {
    /// Commits the generated transcript's provenance before exposing its
    /// stable episode projection. This keeps synthetic episodes on the same
    /// evidence path as workflow-produced transcripts.
    @MainActor
    func commitGeneratedTranscript(
        _ transcript: Transcript,
        for episode: Episode
    ) async throws {
        guard let sharedLibrary = store?.sharedLibrary else {
            throw AgentTTSError.storeUnavailable
        }
        let inputVersion = DesiredStatePlanner.audioVersion(episode)
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.sortedKeys]
        let payloadDigest = ArtifactRepository.hash(try encoder.encode(transcript))
        _ = try sharedLibrary.submitTranscriptObservation(
            transcript,
            context: TranscriptObservationContext(
                podcastID: episode.podcastID,
                sourceRevision: inputVersion,
                sourcePayloadDigest: payloadDigest,
                provider: "pod0AgentComposer"
            )
        )
    }
}
