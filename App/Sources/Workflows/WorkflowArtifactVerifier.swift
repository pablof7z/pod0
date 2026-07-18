import Foundation

@MainActor
final class WorkflowArtifactVerifier: JobPostconditionVerifier {
    private let appStore: AppStateStore
    private let artifacts: ArtifactRepository

    init(appStore: AppStateStore, artifacts: ArtifactRepository) {
        self.appStore = appStore
        self.artifacts = artifacts
    }

    func verifyAndCommit(
        _ job: WorkJob,
        leaseToken: UUID,
        outputVersion: String?
    ) async throws -> Bool {
        guard let outputVersion else { return false }
        guard try isStillCurrent(job) else { return false }
        let records: [ArtifactRecord]
        switch job.kind {
        case .feedDiscovery:
            records = [record(.feedDiscovery, job: job, output: outputVersion, hash: outputVersion)]
        case .transcriptIngest:
            guard let data = TranscriptStore.shared.verifiedStagedData(
                episodeID: job.subjectID, leaseToken: leaseToken
            ),
                  ArtifactRepository.hash(data) == outputVersion,
                  let transcript = try? Self.decoder.decode(Transcript.self, from: data),
                  transcript.episodeID == job.subjectID else { return false }
            let selectedURL = try TranscriptStore.shared.promoteStaged(
                episodeID: job.subjectID,
                leaseToken: leaseToken,
                contentHash: outputVersion
            )
            records = [record(
                .transcript, job: job, output: outputVersion,
                hash: outputVersion,
                location: selectedURL.path,
                origin: Self.projectedSource(transcript.source).rawValue
            )]
        case .transcriptIndex:
            guard TranscriptStore.shared.verifiedData(episodeID: job.subjectID) != nil,
                  let receiptData = Data(base64Encoded: outputVersion),
                  let receipt = try? Self.decoder.decode(
                    VectorArtifactReceipt.self, from: receiptData
                  ),
                  receipt.generation == job.inputVersion,
                  receipt.artifactKind == VectorIndex.semanticArtifactKind,
                  try await RAGService.shared.index.verifyArtifact(
                    episodeID: job.subjectID, receipt: receipt
                  ) else { return false }
            records = [record(
                .semanticIndex,
                job: job,
                output: receipt.generation,
                hash: ArtifactRepository.hash(receiptData),
                origin: outputVersion,
                schemaVersion: receipt.schemaVersion
            )]
        case .metadataIndex:
            guard let receiptData = Data(base64Encoded: outputVersion),
                  let receipt = try? Self.decoder.decode(
                    VectorArtifactReceipt.self, from: receiptData
                  ),
                  receipt.generation == job.inputVersion,
                  receipt.artifactKind == VectorIndex.metadataArtifactKind,
                  try await RAGService.shared.index.verifyArtifact(
                    episodeID: job.subjectID, receipt: receipt
                  ) else { return false }
            records = [record(
                .metadataIndex,
                job: job,
                output: receipt.generation,
                hash: ArtifactRepository.hash(receiptData),
                origin: outputVersion,
                schemaVersion: receipt.schemaVersion
            )]
        case .publisherChapters:
            guard let verified = DerivedArtifactStagingStore.shared.verifiedChapters(
                episodeID: job.subjectID,
                inputVersion: job.inputVersion,
                leaseToken: leaseToken,
                manifestHash: outputVersion
            ), verified.output.chapterOrigin == .publisher else { return false }
            let locations = try DerivedArtifactStagingStore.shared.promote(
                verified,
                episodeID: job.subjectID
            )
            records = [record(
                .chapters,
                job: job,
                output: verified.chaptersHash,
                hash: verified.chaptersHash,
                location: locations.chapters.path,
                origin: DesiredStatePlanner.publisherChapterOrigin(
                    sourceVersion: job.inputVersion,
                    enriched: false
                )
            )]
        case .chapterArtifacts:
            guard let verified = DerivedArtifactStagingStore.shared.verifiedChapters(
                episodeID: job.subjectID,
                inputVersion: job.inputVersion,
                leaseToken: leaseToken,
                manifestHash: outputVersion
            ) else { return false }
            let locations = try DerivedArtifactStagingStore.shared.promote(
                verified, episodeID: job.subjectID
            )
            let chapterOrigin: String
            if verified.output.chapterOrigin == .publisherEnriched,
               let episode = appStore.episode(id: job.subjectID),
               let sourceVersion = DesiredStatePlanner.publisherChapterInputVersion(episode) {
                chapterOrigin = DesiredStatePlanner.publisherChapterOrigin(
                    sourceVersion: sourceVersion,
                    enriched: true
                )
            } else {
                chapterOrigin = verified.output.chapterOrigin.rawValue
            }
            records = [
                record(
                    .chapters, job: job, output: verified.chaptersHash,
                    hash: verified.chaptersHash, location: locations.chapters.path,
                    origin: chapterOrigin
                ),
                record(
                    .adSegments, job: job, output: verified.adsHash,
                    hash: verified.adsHash, location: locations.ads.path,
                    origin: "generated"
                ),
            ]
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
               let data = try? Data(
                contentsOf: URL(fileURLWithPath: location),
                options: .mappedIfSafe
               ), ArtifactRepository.hash(data) == selected.contentHash {
                records = [selected]
            } else {
                guard let staged = EpisodeDownloadStore.shared.verifiedStagedOutput(
                    episodeID: job.subjectID,
                    jobID: job.id,
                    inputVersion: job.inputVersion,
                    contentHash: outputVersion
                ) else { return false }
                let url = try EpisodeDownloadStore.shared.promote(staged, episode: episode)
                records = [record(
                    .downloadFile, job: job, output: staged.contentHash,
                    hash: staged.contentHash, location: url.path, origin: "urlSession"
                )]
            }
        }
        try artifacts.commit(records, completingJobID: job.id, leaseToken: leaseToken)
        if job.kind == .transcriptIndex || job.kind == .metadataIndex,
           let encoded = records.first?.origin,
           let data = Data(base64Encoded: encoded),
           let receipt = try? Self.decoder.decode(VectorArtifactReceipt.self, from: data) {
            try await RAGService.shared.index.selectArtifact(
                episodeID: job.subjectID, receipt: receipt
            )
        }
        for record in records { applyStableProjection(for: record, job: job) }
        return true
    }

    private func isStillCurrent(_ job: WorkJob) throws -> Bool {
        switch job.kind {
        case .download, .transcriptIngest:
            guard let episode = appStore.episode(id: job.subjectID) else { return false }
            return DesiredStatePlanner.audioVersion(episode) == job.inputVersion
        case .metadataIndex, .transcriptIndex, .publisherChapters, .chapterArtifacts:
            let desired = DesiredStatePlanner().plan(.init(
                episodes: appStore.state.episodes,
                settings: appStore.state.settings,
                artifacts: try artifacts.all(),
                transcriptDesiredEpisodeIDs: Set(appStore.state.episodes.map(\.id)),
                scheduledTasks: appStore.scheduledTasks,
                now: Date()
            ))
            return desired.contains { $0.idempotencyKey == job.idempotencyKey }
        case .feedDiscovery, .autoDownload, .newEpisodeNotification, .scheduledAgentRun:
            return true
        }
    }

    private func applyStableProjection(for record: ArtifactRecord, job: WorkJob) {
        switch record.kind {
        case .transcript:
            guard let location = record.location else { return }
            _ = appStore.applyTranscriptEvent(.artifactCommitted(.init(
                inputVersion: record.inputVersion,
                contentHash: record.contentHash,
                fileURL: URL(fileURLWithPath: location),
                source: TranscriptState.Source(rawValue: record.origin ?? "") ?? .other
            )), episodeID: record.subjectID)
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
        case .chapters:
            guard let location = record.location,
                  let chapters = DerivedArtifactStagingStore.shared.loadChapters(
                    at: URL(fileURLWithPath: location)
                  ) else { return }
            appStore.setEpisodeChapters(record.subjectID, chapters: chapters)
        case .adSegments:
            guard let location = record.location,
                  let ads = DerivedArtifactStagingStore.shared.loadAds(
                    at: URL(fileURLWithPath: location)
                  ) else { return }
            appStore.setEpisodeAdSegments(record.subjectID, segments: ads)
        default:
            break
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

    private static func projectedSource(
        _ source: TranscriptSource
    ) -> TranscriptState.Source {
        switch source {
        case .publisher: .publisher
        case .scribeV1: .scribe
        case .whisper: .whisper
        case .onDevice: .onDevice
        case .assemblyAI: .assemblyAI
        }
    }
}
