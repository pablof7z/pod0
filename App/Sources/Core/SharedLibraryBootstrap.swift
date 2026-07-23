import Foundation
import Pod0Core
import os.log

enum SharedLibraryBootstrapOutcome {
    case ready(SharedLibraryClient)
    case authoritativeUnavailable(reason: String, stage: SharedLibraryBootstrapStage)
}

enum SharedLibraryBootstrap {
    private static let logger = Logger.app("SharedLibraryBootstrap")

    @MainActor
    static func run(
        persistence: Persistence,
        legacyState: AppState,
        feedHost: any CoreFeedHosting = CoreFeedHost(),
        chapterCompilationModel: String = Settings().chapterCompilationModel,
        legacyRecallConfiguration: LegacyRecallConfigurationSeed? = nil
    ) -> SharedLibraryBootstrapOutcome {
        persistence.withSharedArtifactMigrationLock {
            runLocked(
                persistence: persistence,
                legacyState: legacyState,
                feedHost: feedHost,
                chapterCompilationModel: chapterCompilationModel,
                legacyRecallConfiguration: legacyRecallConfiguration
            )
        }
    }

    @MainActor
    private static func runLocked(
        persistence: Persistence,
        legacyState: AppState,
        feedHost: any CoreFeedHosting,
        chapterCompilationModel: String,
        legacyRecallConfiguration: LegacyRecallConfigurationSeed?
    ) -> SharedLibraryBootstrapOutcome {
        let target = persistence.sharedCoreStoreURL
        var stage = SharedLibraryBootstrapStage.storePreparation
        do {
            let observedAt = UnixTimestampMilliseconds(date: Date()).value
            let targetSchemaVersion = sharedStoreSchemaVersion()
            let schemaBackup = persistence.sharedCoreSchemaBackupURL(
                targetVersion: targetSchemaVersion
            )
            let storeID = stableID("pod0-core-store:\(target.standardizedFileURL.path)")
            let existingStoreBytes = try? target.resourceValues(
                forKeys: [.fileSizeKey]
            ).fileSize
            let schemaMigrationID = if existingStoreBytes.map({ $0 > 0 }) == true {
                stableID(
                    "pod0-core-schema-migration:"
                        + "\(target.standardizedFileURL.path):v\(targetSchemaVersion)"
                )
            } else {
                storeID
            }
            _ = try prepareSharedListeningStore(
                targetPath: target.path,
                schemaBackupPath: schemaBackup.path,
                migrationId: schemaMigrationID,
                observedAtMilliseconds: observedAt
            )
            stage = .listening
            do {
                _ = try commitStagedLegacyListeningImport(
                    targetPath: target.path,
                    observedAtMilliseconds: observedAt
                )
            } catch LegacyListeningMigrationError.ImportNotFound {
                let plan = try inspectLegacyListeningSource(
                    sourcePath: persistence.episodeStore.fileURL.path
                )
                let importID = stableID(
                    "pod0-listening-import:\(plan.sourceHash):\(plan.sourceGeneration)"
                )
                let report = try stageLegacyListeningImport(
                    sourcePath: persistence.episodeStore.fileURL.path,
                    sourceBackupPath: persistence.legacyListeningBackupURL.path,
                    targetPath: target.path,
                    targetSchemaBackupPath: schemaBackup.path,
                    expectedPlan: plan,
                    importId: importID,
                    targetStoreId: storeID,
                    observedAtMilliseconds: observedAt
                )
                let verification = try readStagedLegacyListeningImport(
                    targetPath: target.path,
                    importId: importID
                )
                guard report.staged,
                      verification.report.plan == plan,
                      verification.snapshot.podcasts.count == Int(plan.podcastCount),
                      verification.snapshot.subscriptions.count == Int(plan.subscriptionCount),
                      verification.snapshot.episodes.count == Int(plan.episodeCount)
                else {
                    throw SharedLibraryBootstrapError.verificationFailed
                }
                _ = try commitStagedLegacyListeningImport(
                    targetPath: target.path,
                    observedAtMilliseconds: observedAt
                )
            }
            stage = .notes
            do {
                _ = try commitStagedLegacyNoteImport(
                    targetPath: target.path,
                    observedAtMilliseconds: observedAt
                )
            } catch LegacyNoteMigrationError.ImportNotFound {
                let plan = try inspectLegacyNoteSource(
                    sourcePath: persistence.episodeStore.fileURL.path
                )
                let importID = stableID(
                    "pod0-note-import:\(plan.sourceHash):\(plan.sourceGeneration)"
                )
                let report = try stageLegacyNoteImport(
                    sourcePath: persistence.episodeStore.fileURL.path,
                    sourceBackupPath: persistence.legacyNotesBackupURL.path,
                    targetPath: target.path,
                    targetSchemaBackupPath: schemaBackup.path,
                    expectedPlan: plan,
                    importId: importID,
                    targetStoreId: storeID,
                    observedAtMilliseconds: observedAt
                )
                let verification = try readStagedLegacyNoteImport(
                    targetPath: target.path,
                    importId: importID
                )
                guard report.staged,
                      verification.report.plan == plan,
                      verification.notes.count == Int(plan.noteCount)
                else {
                    throw SharedLibraryBootstrapError.verificationFailed
                }
                _ = try commitStagedLegacyNoteImport(
                    targetPath: target.path,
                    observedAtMilliseconds: observedAt
                )
            }
            persistence.activateSharedNoteAuthority()
            stage = .clips
            do {
                _ = try commitStagedLegacyClipImport(
                    sourcePath: persistence.episodeStore.fileURL.path,
                    targetPath: target.path,
                    observedAtMilliseconds: observedAt
                )
            } catch LegacyClipMigrationError.ImportNotFound {
                try stageAndCommitClips(
                    persistence: persistence,
                    target: target,
                    schemaBackup: schemaBackup,
                    storeID: storeID,
                    observedAt: observedAt
                )
            } catch LegacyClipMigrationError.SourceChanged {
                try stageAndCommitClips(
                    persistence: persistence,
                    target: target,
                    schemaBackup: schemaBackup,
                    storeID: storeID,
                    observedAt: observedAt
                )
            }
            persistence.activateSharedClipAuthority()
            stage = .transcriptInspection
            if try !sharedTranscriptStoreIsAuthoritative(targetPath: target.path) {
                try stageAndCommitTranscripts(
                    persistence: persistence,
                    target: target,
                    schemaBackup: schemaBackup,
                    storeID: storeID,
                    observedAt: observedAt,
                    stage: &stage
                )
            }
            stage = .chapterInspection
            if !sharedChapterStoreIsAuthoritative(targetPath: target.path) {
                try stageAndCommitChapters(
                    persistence: persistence,
                    target: target,
                    schemaBackup: schemaBackup,
                    storeID: storeID,
                    stage: &stage
                )
            }
            stage = .facade
            let facade = try Pod0Facade.open(storePath: target.path)
            stage = .recallConfiguration
            try importLegacyRecallConfiguration(legacyRecallConfiguration, into: facade)
            let legacyJobStore = JobStore(fileURL: persistence.episodeStore.fileURL)
            stage = .downloadWorkflowCutover
            try LegacyDownloadWorkflowCutover.run(
                facade: facade,
                state: legacyState,
                jobStore: legacyJobStore,
                artifactRepository: ArtifactRepository(
                    fileURL: persistence.episodeStore.fileURL
                ),
                backupURL: persistence.legacyDownloadWorkflowBackupURL
            )
            stage = .transcriptWorkflowCutover
            try LegacyTranscriptWorkflowCutover.run(
                facade: facade,
                state: legacyState,
                jobStore: legacyJobStore,
                backupRoot: persistence.legacyTranscriptWorkflowBackupRootURL
            )
            stage = .modelChapterWorkflowCutover
            try LegacyModelChapterWorkflowCutover.run(
                facade: facade,
                jobStore: legacyJobStore,
                backupRoot: persistence.legacyModelChapterWorkflowBackupRootURL,
                configuredModel: chapterCompilationModel
            )
            let modelCutover = facade.modelChapterCutover()
            guard modelCutover.stage == .authoritative,
                  let modelSourceGeneration = modelCutover.sourceGeneration
            else { throw SharedLibraryBootstrapError.verificationFailed }
            stage = .chapterWorkflowRetirement
            try LegacyPublisherChapterWorkflowRetirement.run(
                jobStore: legacyJobStore,
                backupRoot: persistence.legacyPublisherChapterWorkflowBackupRootURL,
                modelSourceGeneration: modelSourceGeneration
            )
            stage = .scheduledAgentWorkflowCutover
            let legacyChatHistory = try LegacyChatHistorySource()
            try LegacyScheduledAgentWorkflowCutover.run(
                facade: facade,
                persistence: persistence,
                state: legacyState,
                jobStore: legacyJobStore,
                history: legacyChatHistory,
                backupRoot: persistence.legacyScheduledAgentWorkflowBackupRootURL
            )
            stage = .agentHistoryCutover
            try LegacyAgentHistoryCutover.run(
                facade: facade,
                source: legacyChatHistory,
                backupRoot: persistence.legacyAgentHistoryBackupRootURL
            )
            stage = .agentMemoryCutover
            try LegacyAgentMemoryCutover.run(
                facade: facade,
                persistence: persistence,
                state: legacyState,
                backupRoot: persistence.legacyAgentMemoryBackupRootURL
            )
            let observationOutbox = try NativeHostObservationOutbox(
                fileURL: persistence.nativeHostObservationOutboxURL
            )
            CoreDownloadHost.shared.configure(coreStoreURL: target)
            let client = SharedLibraryClient(
                facade: facade,
                coreStoreURL: target,
                feedHost: feedHost,
                downloadHost: CoreDownloadHost.shared,
                observationOutbox: observationOutbox
            )
            client.start()
            persistence.activateSharedListeningAuthority()
            logger.info("Shared Rust library is authoritative at \(target.path, privacy: .public)")
            return .ready(client)
        } catch {
            let code = SharedLibraryBootstrapFailureCode.classify(error)
            logger.error("Shared library bootstrap failed at \(stage.rawValue, privacy: .public): \(code.rawValue, privacy: .public)")
            return .authoritativeUnavailable(reason: code.rawValue, stage: stage)
        }
    }

}

enum SharedLibraryBootstrapError: Error {
    case verificationFailed
}
