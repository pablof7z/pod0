import Foundation
import Pod0Core

enum LegacyTranscriptWorkflowSnapshotError: Error, Equatable {
    case duplicateLiveRows(UUID)
    case invalidSource(UUID)
}

struct LegacyTranscriptWorkflowSnapshot: Equatable {
    let sourceGeneration: UInt64
    let sourceFingerprint: ContentDigest
    let candidates: [LegacyTranscriptWorkflowCutoverCandidate]
    let backup: LegacyTranscriptWorkflowBackupManifest

    static func capture(
        facade: Pod0Facade,
        state: AppState,
        jobStore: JobStore
    ) throws -> Self {
        let jobs = try jobStore.legacyTranscriptJobs()
        let episodes = Dictionary(uniqueKeysWithValues: state.episodes.map { ($0.id, $0) })
        let recall = recallConfiguration(facade)
        var candidates: [LegacyTranscriptWorkflowCutoverCandidate] = []
        var classifications: [UUID: LegacyTranscriptWorkflowBackupClassification] = [:]

        for (episodeID, episodeJobs) in Dictionary(grouping: jobs, by: \.subjectID) {
            guard let episode = episodes[episodeID] else { continue }
            let sourceRevision = DesiredStatePlanner.audioVersion(episode)
            let summary = selectedSummary(facade, episodeID: episodeID)
                .flatMap { $0.sourceRevision == sourceRevision ? $0 : nil }
            guard let selected = try select(
                jobs: episodeJobs,
                sourceRevision: sourceRevision,
                summary: summary,
                recall: recall,
                episodeID: episodeID
            ) else { continue }
            let payload = payload(from: selected.job)
            let provider = payload?.provider ?? episode.requestedTranscriptProvider
                ?? state.settings.sttProvider
            let configuration = NativeTranscriptWorkflowConfiguration.make(
                episode: episode,
                settings: state.settings,
                provider: provider
            )
            let disposition = LegacyTranscriptWorkflowDispositionMapper.map(
                selected,
                configuration: configuration
            )
            let classification = disposition.classification
            classifications[selected.job.id] = classification
            candidates.append(LegacyTranscriptWorkflowCutoverCandidate(
                episodeId: EpisodeId(uuid: episodeID),
                sourceRevision: sourceRevision,
                origin: payload?.userInitiated == true ? .user : .automatic,
                configuration: configuration,
                disposition: disposition
            ))
        }

        let backupRows = jobs.map { job in
            LegacyTranscriptWorkflowBackupRow(
                job: job,
                classification: classifications[job.id] ?? .obsolete
            )
        }
        let coreRows = try backupRows.map { try $0.coreValue() }
        let fingerprint = LegacyTranscriptWorkflowBackupManifest.sourceFingerprint(for: coreRows)
        let generation = LegacyTranscriptWorkflowBackupManifest.sourceGeneration(for: fingerprint)
        return Self(
            sourceGeneration: generation,
            sourceFingerprint: fingerprint,
            candidates: candidates.sorted {
                $0.episodeId.high == $1.episodeId.high
                    ? $0.episodeId.low < $1.episodeId.low
                    : $0.episodeId.high < $1.episodeId.high
            },
            backup: LegacyTranscriptWorkflowBackupManifest(
                sourceGeneration: generation,
                sourceFingerprint: fingerprint.stableString,
                rows: backupRows
            )
        )
    }
}

struct LegacyTranscriptWorkflowSelection {
    let job: LegacyTranscriptWorkflowJob
    let selectedTranscriptExists: Bool
    let evidenceInputVersion: String?
}

private extension LegacyTranscriptWorkflowSnapshot {
    static func select(
        jobs: [LegacyTranscriptWorkflowJob],
        sourceRevision: String,
        summary: TranscriptSummaryProjection?,
        recall: RecallConfiguration?,
        episodeID: UUID
    ) throws -> LegacyTranscriptWorkflowSelection? {
        let ingest = jobs.filter {
            $0.kind == .transcriptIngest && $0.inputVersion == sourceRevision
                && $0.state != .obsolete
        }
        if let summary {
            let expectedEvidence = recall.map {
                ArtifactRepository.version(parts: [
                    summary.transcriptVersionId.stableString,
                    summary.transcriptContentDigest.stableString,
                    $0.embeddingSpaceId.stableString,
                    "rust-evidence-v1",
                    "core-recall-index-v1",
                ])
            }
            let legacyEvidence = recall.map {
                ArtifactRepository.version(parts: [
                    summary.transcriptContentDigest.stableString,
                    $0.embeddingSpaceId.stableString,
                    "rust-evidence-v1",
                    "core-recall-index-v1",
                ])
            }
            let index = jobs.filter {
                guard $0.kind == .transcriptIndex, $0.state != .obsolete else { return false }
                return $0.inputVersion == expectedEvidence || $0.inputVersion == legacyEvidence
            }
            if let selected = try selectedRow(index, episodeID: episodeID) {
                return LegacyTranscriptWorkflowSelection(
                    job: selected,
                    selectedTranscriptExists: true,
                    evidenceInputVersion: expectedEvidence
                )
            }
            if let selected = try selectedRow(ingest, episodeID: episodeID) {
                return LegacyTranscriptWorkflowSelection(
                    job: selected,
                    selectedTranscriptExists: true,
                    evidenceInputVersion: nil
                )
            }
            return nil
        }
        guard let selected = try selectedRow(ingest, episodeID: episodeID) else { return nil }
        return LegacyTranscriptWorkflowSelection(
            job: selected,
            selectedTranscriptExists: false,
            evidenceInputVersion: nil
        )
    }

    static func selectedRow(
        _ jobs: [LegacyTranscriptWorkflowJob],
        episodeID: UUID
    ) throws -> LegacyTranscriptWorkflowJob? {
        let live = jobs.filter { $0.state.isActive }
        guard live.count <= 1 else {
            throw LegacyTranscriptWorkflowSnapshotError.duplicateLiveRows(episodeID)
        }
        return (live.first ?? jobs.max { lhs, rhs in
            if lhs.updatedAt != rhs.updatedAt { return lhs.updatedAt < rhs.updatedAt }
            return lhs.id.uuidString < rhs.id.uuidString
        })
    }

    static func payload(from job: LegacyTranscriptWorkflowJob) -> TranscriptJobPayload? {
        guard job.kind == .transcriptIngest,
              let data = job.payload else { return nil }
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return try? decoder.decode(TranscriptJobPayload.self, from: data)
    }

    static func selectedSummary(
        _ facade: Pod0Facade,
        episodeID: UUID
    ) -> TranscriptSummaryProjection? {
        let envelope = facade.snapshot(request: ProjectionRequest(
            scope: .transcript(episodeId: EpisodeId(uuid: episodeID), scope: .summary),
            offset: 0,
            maxItems: 1
        ))
        guard case .transcript(let projection) = envelope.projection,
              projection.failure == nil else { return nil }
        return projection.summary
    }

    static func recallConfiguration(_ facade: Pod0Facade) -> RecallConfiguration? {
        let envelope = facade.snapshot(request: ProjectionRequest(
            scope: .recallConfiguration,
            offset: 0,
            maxItems: 1
        ))
        guard case .recallConfiguration(let value) = envelope.projection else { return nil }
        return value
    }
}
