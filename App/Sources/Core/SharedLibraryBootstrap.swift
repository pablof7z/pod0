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
            let schemaBackup = persistence.sharedCoreSchemaBackupURL(
                targetVersion: sharedStoreSchemaVersion()
            )
            let storeID = stableID("pod0-core-store:\(target.standardizedFileURL.path)")
            _ = try prepareSharedListeningStore(
                targetPath: target.path,
                schemaBackupPath: schemaBackup.path,
                migrationId: storeID,
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
            logger.info("Shared Rust library is authoritative at \(target.path, privacy: .public)")
            return .ready(client)
        } catch {
            let code = SharedLibraryBootstrapFailureCode.classify(error)
            logger.error("Shared library bootstrap failed at \(stage.rawValue, privacy: .public): \(code.rawValue, privacy: .public)")
            return .authoritativeUnavailable(reason: code.rawValue, stage: stage)
        }
    }

    private static func stageAndCommitTranscripts(
        persistence: Persistence,
        target: URL,
        schemaBackup: URL,
        storeID: CommandId,
        observedAt: Int64,
        stage: inout SharedLibraryBootstrapStage
    ) throws {
        let source = persistence.episodeStore.fileURL
        let transcriptRoot = persistence.legacyTranscriptRootURL
        let backupRoot = persistence.legacyTranscriptBackupRootURL
        let plan = try inspectLegacyTranscriptSource(
            sourceDatabasePath: source.path,
            transcriptRootPath: transcriptRoot.path
        )
        var active = try readActiveLegacyTranscriptImport(targetPath: target.path)
        if let existing = active,
           existing.plan != plan || existing.state == .corrupt {
            _ = try discardStagedLegacyTranscriptImport(
                targetPath: target.path,
                importId: existing.importId,
                observedAtMilliseconds: observedAt
            )
            active = nil
        }
        let importID = active?.importId ?? CommandId(uuid: UUID())
        stage = .transcriptStaging
        let report = try stageLegacyTranscriptImport(
            sourceDatabasePath: source.path,
            transcriptRootPath: transcriptRoot.path,
            legacyBackupRootPath: backupRoot.path,
            targetPath: target.path,
            targetSchemaBackupPath: schemaBackup.path,
            expectedPlan: plan,
            importId: importID,
            targetStoreId: storeID,
            observedAtMilliseconds: observedAt
        )
        stage = .transcriptVerification
        let verification = try verifyStagedLegacyTranscriptImport(
            targetPath: target.path,
            legacyBackupRootPath: backupRoot.path,
            importId: importID,
            observedAtMilliseconds: observedAt
        )
        guard report.plan == plan,
              verification.report.plan == plan,
              verification.verifiedArtifactCount == plan.artifactCount
        else {
            throw SharedLibraryBootstrapError.verificationFailed
        }
        stage = .transcriptCommit
        let committed = try commitStagedLegacyTranscriptImport(
            sourceDatabasePath: source.path,
            transcriptRootPath: transcriptRoot.path,
            targetPath: target.path,
            importId: importID,
            observedAtMilliseconds: observedAt
        )
        guard committed.state == .committed,
              try sharedTranscriptStoreIsAuthoritative(targetPath: target.path)
        else {
            throw SharedLibraryBootstrapError.verificationFailed
        }
    }

}

enum SharedLibraryBootstrapError: Error {
    case verificationFailed
}
