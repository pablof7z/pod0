import Foundation

extension EpisodeDownloadService {
    func verifiedExistingDownloadHash(
        _ episode: Episode,
        context: JobAttemptContext
    ) async -> String? {
        guard case let .downloaded(_, byteCount) = episode.downloadState,
              EpisodeDownloadStore.shared.exists(for: episode) else { return nil }
        let result = await ArtifactVerificationExecutor.shared.verify(.init(
            artifactID: "download:\(episode.id.uuidString)",
            location: EpisodeDownloadStore.shared.localFileURL(for: episode),
            expectedHash: nil,
            expectedSize: byteCount,
            schemaVersion: 1,
            cancellationID: context.leaseToken
        ))
        return result.isAvailable ? result.observedHash : nil
    }

    func verifiedStagedDownloadHash(context: JobAttemptContext) async -> String? {
        await ArtifactVerificationExecutor.shared.verifiedStagedDownload(
            episodeID: context.job.subjectID,
            jobID: context.job.id,
            inputVersion: context.job.inputVersion
        )?.contentHash
    }
}
