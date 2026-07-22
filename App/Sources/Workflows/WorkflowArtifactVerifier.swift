import Foundation

@MainActor
final class WorkflowArtifactVerifier: JobPostconditionVerifier {
    private let artifacts: ArtifactRepository

    init(artifacts: ArtifactRepository) {
        self.artifacts = artifacts
    }

    func verifyAndCommit(
        _ job: WorkJob,
        leaseToken: UUID,
        outputVersion: String?
    ) async throws -> Bool {
        guard let outputVersion else { return false }
        guard isStillCurrent(job) else { return false }
        let records: [ArtifactRecord]
        switch job.kind {
        case .feedDiscovery:
            records = [record(.feedDiscovery, job: job, output: outputVersion, hash: outputVersion)]
        case .transcriptIngest, .transcriptIndex:
            return false
        case .metadataIndex:
            return false
        case .autoDownload, .download:
            return false
        case .newEpisodeNotification:
            records = [record(.notificationDelivery, job: job, output: outputVersion, hash: outputVersion)]
        case .scheduledAgentRun:
            return false
        }
        try artifacts.commit(records, completingJobID: job.id, leaseToken: leaseToken)
        return true
    }

    func isStillCurrent(_ job: WorkJob) -> Bool {
        switch job.kind {
        case .transcriptIngest, .transcriptIndex, .scheduledAgentRun:
            return false
        case .metadataIndex:
            return false
        case .feedDiscovery, .download, .autoDownload, .newEpisodeNotification:
            return true
        }
    }

    private func record(
        _ kind: ArtifactKind,
        job: WorkJob,
        output: String,
        hash: String,
        location: String? = nil,
        origin: String? = nil,
        schemaVersion: Int = 1
    ) -> ArtifactRecord {
        ArtifactRecord(
            kind: kind, subjectID: job.subjectID,
            inputVersion: job.inputVersion, outputVersion: output,
            contentHash: hash, location: location, origin: origin,
            schemaVersion: schemaVersion, integrity: .available, verifiedAt: Date()
        )
    }

}
