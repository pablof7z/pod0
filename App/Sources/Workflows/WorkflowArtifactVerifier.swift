import Foundation

@MainActor
final class WorkflowArtifactVerifier: JobPostconditionVerifier {
    private let appStore: AppStateStore
    private let artifacts: ArtifactRepository
    private let fileVerifier: ArtifactVerificationExecutor

    init(
        appStore: AppStateStore,
        artifacts: ArtifactRepository,
        fileVerifier: ArtifactVerificationExecutor = .shared
    ) {
        self.appStore = appStore
        self.artifacts = artifacts
        self.fileVerifier = fileVerifier
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
        case .publisherChapters:
            return false
        case .chapterArtifacts:
            guard let receipt = chapterReceipt(outputVersion),
                  receipt.episodeID == job.subjectID,
                  receipt.inputVersion == job.inputVersion,
                  appStore.sharedLibrary?.verifyChapterWorkflowReceipt(receipt) == true
            else { return false }
            try artifacts.completeWithoutArtifact(
                outputVersion: outputVersion,
                completingJobID: job.id,
                leaseToken: leaseToken
            )
            return true
        case .autoDownload:
            records = [record(.autoDownloadDecision, job: job, output: outputVersion, hash: outputVersion)]
        case .newEpisodeNotification:
            records = [record(.notificationDelivery, job: job, output: outputVersion, hash: outputVersion)]
        case .scheduledAgentRun:
            guard let occurrenceID = job.occurrenceID,
                  occurrenceID == outputVersion,
                  ChatHistoryStore.shared.conversation(
                    occurrenceID: occurrenceID
                  )?.hasCompletedScheduledOutput == true else { return false }
            records = [record(.scheduledOutput, job: job, output: outputVersion, hash: outputVersion)]
        case .download:
            guard let episode = appStore.episode(id: job.subjectID) else { return false }
            if let selected = try artifacts.current(
                kind: .downloadFile,
                subjectID: job.subjectID
            ), selected.integrity == .available,
               selected.inputVersion == job.inputVersion,
               selected.contentHash == outputVersion,
               let location = selected.location,
               await fileVerifier.verify(.init(
                artifactID: "download:\(job.subjectID.uuidString)",
                location: URL(fileURLWithPath: location),
                expectedHash: selected.contentHash,
                expectedSize: nil,
                schemaVersion: selected.schemaVersion,
                cancellationID: leaseToken
               )).isAvailable {
                records = [selected]
            } else {
                guard let staged = await fileVerifier.verifiedStagedDownload(
                    episodeID: job.subjectID,
                    jobID: job.id,
                    inputVersion: job.inputVersion,
                    contentHash: outputVersion
                ) else { return false }
                let url = try await fileVerifier.promoteDownload(staged, episode: episode)
                records = [record(
                    .downloadFile, job: job, output: staged.contentHash,
                    hash: staged.contentHash, location: url.path, origin: "urlSession"
                )]
            }
        }
        try artifacts.commit(records, completingJobID: job.id, leaseToken: leaseToken)
        for record in records { await applyStableProjection(for: record, job: job) }
        return true
    }

    func isStillCurrent(_ job: WorkJob) -> Bool {
        switch job.kind {
        case .download, .transcriptIngest:
            guard let episode = appStore.episode(id: job.subjectID) else { return false }
            return DesiredStatePlanner.audioVersion(episode) == job.inputVersion
        case .metadataIndex:
            return false
        case .transcriptIndex:
            guard let episode = appStore.episode(id: job.subjectID),
                  let transcript = transcriptSnapshot(episodeID: episode.id),
                  transcript.sourceRevision == DesiredStatePlanner.audioVersion(episode)
            else { return false }
            return DesiredStatePlanner.transcriptIndexInputVersion(
                transcript,
                settings: appStore.state.settings
            ) == job.inputVersion
        case .publisherChapters:
            return false
        case .chapterArtifacts:
            guard let episode = appStore.episode(id: job.subjectID),
                  let transcript = transcriptSnapshot(episodeID: episode.id),
                  transcript.sourceRevision == DesiredStatePlanner.audioVersion(episode)
            else { return false }
            guard let sharedLibrary = appStore.sharedLibrary,
                  case .ready(let request) = sharedLibrary.chapterModelPlan(
                    episodeID: episode.id,
                    configuredModel: appStore.state.settings.chapterCompilationModel
                  )
            else { return false }
            return request.sourceVersion == job.inputVersion
        case .feedDiscovery, .autoDownload, .newEpisodeNotification, .scheduledAgentRun:
            return true
        }
    }

    private func transcriptSnapshot(episodeID: UUID) -> TranscriptWorkflowSnapshot? {
        appStore.sharedLibrary?.transcriptWorkflowSnapshots(
            episodeIDs: [episodeID]
        ).first
    }

    private func applyStableProjection(for record: ArtifactRecord, job: WorkJob) async {
        switch record.kind {
        case .downloadFile:
            guard let location = record.location,
                  let attributes = try? FileManager.default.attributesOfItem(atPath: location),
                  let size = attributes[.size] as? NSNumber
            else { return }
            _ = appStore.applyDownloadEvent(.artifactCommitted(.init(
                inputVersion: record.inputVersion,
                contentHash: record.contentHash,
                fileURL: URL(fileURLWithPath: location),
                byteCount: size.int64Value
            )), episodeID: record.subjectID)
        default:
            break
        }
    }

    private func chapterReceipt(_ outputVersion: String) -> SharedChapterWorkflowReceipt? {
        guard let data = Data(base64Encoded: outputVersion) else { return nil }
        return try? Self.decoder.decode(SharedChapterWorkflowReceipt.self, from: data)
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
