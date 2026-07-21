import Foundation
import Pod0Core

enum LegacyModelChapterWorkflowCutoverError: Error, Equatable {
    case blocked(String)
    case missingSourceGeneration
    case legacyRowsRemain
    case commitFailed
}

enum LegacyModelChapterWorkflowCutover {
    static func run(
        facade: Pod0Facade,
        jobStore: JobStore,
        backupRoot: URL,
        configuredModel: String
    ) throws {
        var projection = facade.modelChapterCutover()
        var captured: LegacyModelChapterWorkflowSnapshot?
        if projection.stage == .notStarted {
            let snapshot = try LegacyModelChapterWorkflowSnapshot.capture(from: jobStore)
            captured = snapshot
            projection = stage(snapshot, facade: facade, configuredModel: configuredModel)
        }
        guard projection.stage == .staged || projection.stage == .authoritative else {
            throw LegacyModelChapterWorkflowCutoverError.blocked(
                projection.failure?.diagnosticCode ?? "model_chapter_cutover_blocked"
            )
        }
        guard var sourceGeneration = projection.sourceGeneration else {
            throw LegacyModelChapterWorkflowCutoverError.missingSourceGeneration
        }

        if projection.stage == .staged {
            var jobs = try legacyJobs(in: jobStore)
            if try !sourceMatchesStage(
                jobs: jobs,
                captured: captured,
                sourceGeneration: sourceGeneration,
                backupRoot: backupRoot,
                jobStore: jobStore
            ) {
                guard !jobs.isEmpty else {
                    throw LegacyModelChapterWorkflowBackupError.backupMissing
                }
                let discarded = facade.discardStagedLegacyModelChapterCutover(
                    sourceGeneration: sourceGeneration
                )
                guard discarded.stage == .notStarted else {
                    throw LegacyModelChapterWorkflowCutoverError.blocked(
                        discarded.failure?.diagnosticCode ?? "model_chapter_cutover_discard_blocked"
                    )
                }
                let refreshed = try LegacyModelChapterWorkflowSnapshot.capture(from: jobStore)
                projection = stage(
                    refreshed,
                    facade: facade,
                    configuredModel: configuredModel
                )
                guard projection.stage == .staged,
                      let refreshedGeneration = projection.sourceGeneration else {
                    throw LegacyModelChapterWorkflowCutoverError.blocked(
                        projection.failure?.diagnosticCode ?? "model_chapter_cutover_restage_blocked"
                    )
                }
                sourceGeneration = refreshedGeneration
                jobs = try legacyJobs(in: jobStore)
                guard try sourceMatchesStage(
                    jobs: jobs,
                    captured: refreshed,
                    sourceGeneration: sourceGeneration,
                    backupRoot: backupRoot,
                    jobStore: jobStore
                ) else {
                    throw LegacyModelChapterWorkflowBackupError.sourceChanged
                }
            }
            guard try jobStore.removeJobs(
                kind: .chapterArtifacts,
                matching: jobs
            ) else {
                throw LegacyModelChapterWorkflowBackupError.sourceChanged
            }
            guard try jobStore.allJobs().allSatisfy({ $0.kind != .chapterArtifacts }) else {
                throw LegacyModelChapterWorkflowCutoverError.legacyRowsRemain
            }
            projection = facade.commitLegacyModelChapterCutover(
                sourceGeneration: sourceGeneration
            )
        } else {
            _ = try LegacyModelChapterWorkflowBackupManifest.load(
                from: backupRoot,
                sourceGeneration: sourceGeneration
            )
            guard try jobStore.allJobs().allSatisfy({ $0.kind != .chapterArtifacts }) else {
                throw LegacyModelChapterWorkflowCutoverError.legacyRowsRemain
            }
        }
        guard projection.stage == .authoritative,
              projection.sourceGeneration == sourceGeneration else {
            throw LegacyModelChapterWorkflowCutoverError.commitFailed
        }
    }

    private static func stage(
        _ snapshot: LegacyModelChapterWorkflowSnapshot,
        facade: Pod0Facade,
        configuredModel: String
    ) -> LegacyModelChapterCutoverProjection {
        facade.stageLegacyModelChapterCutover(
            sourceGeneration: snapshot.sourceGeneration,
            configuredModel: configuredModel,
            candidates: snapshot.candidates
        )
    }

    private static func legacyJobs(in store: JobStore) throws -> [WorkJob] {
        try store.allJobs()
            .filter { $0.kind == .chapterArtifacts }
            .sorted { $0.id.uuidString < $1.id.uuidString }
    }

    private static func sourceMatchesStage(
        jobs: [WorkJob],
        captured: LegacyModelChapterWorkflowSnapshot?,
        sourceGeneration: UInt64,
        backupRoot: URL,
        jobStore: JobStore
    ) throws -> Bool {
        if let backup = try LegacyModelChapterWorkflowBackupManifest.load(
            from: backupRoot,
            sourceGeneration: sourceGeneration,
            required: false
        ) {
            return jobs.isEmpty || backup.matches(jobs)
        }
        let snapshot = try captured
            ?? LegacyModelChapterWorkflowSnapshot.capture(from: jobStore)
        guard snapshot.sourceGeneration == sourceGeneration,
              snapshot.backup.matches(jobs) else { return false }
        try snapshot.backup.publish(to: backupRoot)
        return true
    }
}
