import Foundation
import Pod0Core

enum LegacyDownloadWorkflowCutoverError: Error {
    case verificationFailed
}

enum LegacyDownloadWorkflowCutover {
    @MainActor
    static func run(
        facade: Pod0Facade,
        state: AppState,
        jobStore: JobStore,
        artifactRepository: ArtifactRepository,
        backupURL: URL,
        nativeStore: CoreDownloadNativeStore = CoreDownloadNativeStore()
    ) throws {
        let current = facade.downloadCutover()
        if current.stage == .authoritative {
            guard let generation = current.sourceGeneration else {
                throw LegacyDownloadWorkflowCutoverError.verificationFailed
            }
            if FileManager.default.fileExists(atPath: backupURL.path) {
                let backup = try LegacyDownloadWorkflowBackup.load(from: backupURL)
                guard LegacyDownloadWorkflowBackup.storageGeneration(
                    backup.sourceGeneration
                ) == generation
                else { throw LegacyDownloadWorkflowCutoverError.verificationFailed }
                if try !jobStore.legacyDownloadWorkflowsAreRetired() {
                    guard try jobStore.retireLegacyDownloadWorkflows(matching: backup),
                          try jobStore.legacyDownloadWorkflowsAreRetired()
                    else { throw LegacyDownloadWorkflowCutoverError.verificationFailed }
                }
            } else if try !jobStore.legacyDownloadWorkflowsAreRetired() {
                throw LegacyDownloadWorkflowCutoverError.verificationFailed
            }
            return
        }

        let source: LegacyDownloadWorkflowSnapshot
        if current.stage == .staged {
            let backup = try LegacyDownloadWorkflowBackup.load(from: backupURL)
            source = try restoreSnapshot(backup: backup, state: state)
        } else {
            guard current.stage == .notStarted else {
                throw LegacyDownloadWorkflowCutoverError.verificationFailed
            }
            let retired = LegacyDownloadSessionRetirement.shared.captureAndCancel()
            let episodes = Dictionary(uniqueKeysWithValues: state.episodes.map { ($0.id, $0) })
            for (episodeID, data) in retired.resumeData {
                if let episode = episodes[episodeID] {
                    try LegacyDownloadSourceStore.shared.writeResumeData(
                        data,
                        episodeID: episode.id
                    )
                }
            }
            let captured = try LegacyDownloadWorkflowSnapshot.capture(
                state: state,
                jobStore: jobStore,
                artifactRepository: artifactRepository,
                tasks: retired.tasks,
                producedResumeData: retired.resumeData
            )
            if FileManager.default.fileExists(atPath: backupURL.path) {
                let existing = try LegacyDownloadWorkflowBackup.load(from: backupURL)
                guard existing.normalizedForStorage() == captured.backup else {
                    throw LegacyDownloadWorkflowBackupError.sourceChanged
                }
                source = try restoreSnapshot(backup: existing, state: state)
            } else {
                source = captured
                try source.backup.publish(to: backupURL)
            }
        }

        let staged = facade.stageLegacyDownloadCutover(
            sourceGeneration: source.sourceGeneration,
            candidates: source.candidates
        )
        guard staged.stage == .staged,
              staged.sourceGeneration == source.sourceGeneration,
              Int(staged.adoptedAvailable + staged.scheduledRestart) == source.candidates.count
        else { throw LegacyDownloadWorkflowCutoverError.verificationFailed }

        let attempts = downloadAttempts(facade: facade)
        do {
            for (episodeID, data) in source.resumeDataByEpisodeID {
                guard let attempt = attempts[episodeID] else {
                    throw LegacyDownloadWorkflowCutoverError.verificationFailed
                }
                try nativeStore.importLegacyResumeData(data, for: attempt)
            }
            let committed = facade.commitLegacyDownloadCutover(
                sourceGeneration: source.sourceGeneration
            )
            guard committed.stage == .authoritative,
                  committed.sourceGeneration == source.sourceGeneration,
                  try jobStore.retireLegacyDownloadWorkflows(matching: source.backup),
                  try jobStore.legacyDownloadWorkflowsAreRetired()
            else { throw LegacyDownloadWorkflowCutoverError.verificationFailed }
        } catch {
            if facade.downloadCutover().stage == .staged {
                _ = facade.discardStagedLegacyDownloadCutover(
                    sourceGeneration: source.sourceGeneration,
                    candidates: source.candidates
                )
                for attempt in attempts.values { nativeStore.removeNativeFiles(for: attempt) }
            }
            throw error
        }
    }

    private static func downloadAttempts(facade: Pod0Facade) -> [UUID: DownloadAttemptId] {
        var result: [UUID: DownloadAttemptId] = [:]
        var offset: UInt32 = 0
        while true {
            let envelope = facade.snapshot(request: ProjectionRequest(
                scope: .downloads(episodeId: nil),
                offset: offset,
                maxItems: 200
            ))
            guard case .downloads(let page) = envelope.projection else { break }
            for workflow in page.workflows {
                if let episodeID = workflow.episodeId.uuid, let attempt = workflow.attemptId {
                    result[episodeID] = attempt
                }
            }
            guard page.hasMore, offset <= UInt32.max - 200 else { break }
            offset += 200
        }
        return result
    }

    private static func restoreSnapshot(
        backup: LegacyDownloadWorkflowBackup,
        state: AppState
    ) throws -> LegacyDownloadWorkflowSnapshot {
        let episodes = Dictionary(uniqueKeysWithValues: state.episodes.map { ($0.id, $0) })
        let candidates = backup.candidates.map { evidence in
            LegacyDownloadCutoverCandidate(
                episodeId: EpisodeId(uuid: evidence.episodeID),
                origin: evidence.origin.coreValue,
                disposition: evidence.disposition == .available
                    ? .available(
                        sourcePath: evidence.sourcePath ?? "",
                        byteCount: UInt64(max(0, evidence.byteCount ?? 0))
                    )
                    : .restart(resumeAvailable: evidence.resumeByteCount.map { $0 > 0 } ?? false)
            )
        }
        let resumePairs: [(UUID, Data)] = backup.candidates.compactMap { evidence in
            guard evidence.disposition == .restart,
                  let episode = episodes[evidence.episodeID],
                  let data = LegacyDownloadSourceStore.shared.loadResumeData(
                    episodeID: episode.id
                  ),
                  LegacyDownloadWorkflowBackup.digest(data) == evidence.resumeDigest
            else { return nil }
            return (evidence.episodeID, data)
        }
        let resumeData = Dictionary(uniqueKeysWithValues: resumePairs)
        return LegacyDownloadWorkflowSnapshot(
            sourceGeneration: LegacyDownloadWorkflowBackup.storageGeneration(
                backup.sourceGeneration
            ),
            candidates: candidates,
            resumeDataByEpisodeID: resumeData,
            backup: backup
        )
    }
}
