import Foundation

@MainActor
final class WorkflowArtifactVerifier: JobPostconditionVerifier {
    private let appStore: AppStateStore
    private let artifacts: ArtifactRepository

    init(
        appStore: AppStateStore,
        artifacts: ArtifactRepository
    ) {
        self.appStore = appStore
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
        case .transcriptIngest:
            guard let receiptData = Data(base64Encoded: outputVersion),
                  let receipt = try? Self.decoder.decode(
                    SharedTranscriptWorkflowReceipt.self,
                    from: receiptData
                  ),
                  receipt.episodeID == job.subjectID,
                  receipt.inputVersion == job.inputVersion,
                  appStore.sharedLibrary?.verifyTranscriptWorkflowReceipt(receipt) == true
            else { return false }
            try artifacts.completeWithoutArtifact(
                outputVersion: outputVersion,
                completingJobID: job.id,
                leaseToken: leaseToken
            )
            return true
        case .transcriptIndex:
            guard let receiptData = Data(base64Encoded: outputVersion),
                  let receipt = try? Self.decoder.decode(
                    SharedEvidenceReceipt.self, from: receiptData
                  ),
                  receipt.episodeID == job.subjectID,
                  receipt.inputVersion == job.inputVersion,
                  appStore.sharedLibrary?.verifyEvidenceReceipt(receipt) == true else { return false }
            records = [record(
                .semanticIndex,
                job: job,
                output: receipt.generationID,
                hash: receipt.transcriptContentDigest,
                origin: outputVersion,
                schemaVersion: receipt.schemaVersion
            )]
        case .metadataIndex:
            return false
        case .autoDownload, .download:
            return false
        case .newEpisodeNotification:
            records = [record(.notificationDelivery, job: job, output: outputVersion, hash: outputVersion)]
        case .scheduledAgentRun:
            guard let occurrenceID = job.occurrenceID,
                  occurrenceID == outputVersion,
                  ChatHistoryStore.shared.conversation(
                    occurrenceID: occurrenceID
                  )?.hasCompletedScheduledOutput == true else { return false }
            records = [record(.scheduledOutput, job: job, output: outputVersion, hash: outputVersion)]
        }
        try artifacts.commit(records, completingJobID: job.id, leaseToken: leaseToken)
        return true
    }

    func isStillCurrent(_ job: WorkJob) -> Bool {
        switch job.kind {
        case .transcriptIngest:
            guard let episode = appStore.episode(id: job.subjectID) else { return false }
            return DesiredStatePlanner.audioVersion(episode) == job.inputVersion
        case .metadataIndex:
            return false
        case .transcriptIndex:
            guard let episode = appStore.episode(id: job.subjectID),
                  let transcript = transcriptSnapshot(episodeID: episode.id),
                  transcript.sourceRevision == DesiredStatePlanner.audioVersion(episode)
            else { return false }
            guard let embeddingSpaceID = appStore.recallConfiguration?
                .embeddingSpaceId.stableString else { return false }
            return DesiredStatePlanner.transcriptIndexInputVersion(
                transcript,
                embeddingSpaceID: embeddingSpaceID
            ) == job.inputVersion
        case .feedDiscovery, .download, .autoDownload,
             .newEpisodeNotification, .scheduledAgentRun:
            return true
        }
    }

    private func transcriptSnapshot(episodeID: UUID) -> TranscriptWorkflowSnapshot? {
        appStore.sharedLibrary?.transcriptWorkflowSnapshots(
            episodeIDs: [episodeID]
        ).first
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

    private static let encoder: JSONEncoder = {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.sortedKeys]
        return encoder
    }()

    private static let decoder: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return decoder
    }()

}
