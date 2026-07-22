import Foundation
import Pod0Core

enum LegacyTranscriptWorkflowCutoverError: Error, Equatable {
    case blocked(String)
    case missingSourceGeneration
    case sourceChanged
    case legacyRowsRemain
    case commitFailed
}

enum LegacyTranscriptWorkflowCutover {
    @MainActor
    static func run(
        facade: Pod0Facade,
        state: AppState,
        jobStore: JobStore,
        backupRoot: URL
    ) throws {
        var projection = facade.transcriptWorkflowCutover()
        if projection.stage == .notStarted {
            projection = try stage(
                LegacyTranscriptWorkflowSnapshot.capture(
                    facade: facade,
                    state: state,
                    jobStore: jobStore
                ),
                facade: facade,
                backupRoot: backupRoot
            )
        }
        guard projection.stage == .staged
                || projection.stage == .verified
                || projection.stage == .authoritative else {
            throw LegacyTranscriptWorkflowCutoverError.blocked(
                projection.failure?.diagnosticCode ?? "transcript_workflow_cutover_blocked"
            )
        }
        guard var generation = projection.sourceGeneration else {
            throw LegacyTranscriptWorkflowCutoverError.missingSourceGeneration
        }

        if projection.stage == .authoritative {
            _ = try LegacyTranscriptWorkflowBackupManifest.load(
                from: backupRoot,
                sourceGeneration: generation
            )
            guard try jobStore.legacyTranscriptJobsAreRetired() else {
                throw LegacyTranscriptWorkflowCutoverError.legacyRowsRemain
            }
            return
        }

        var backup = try LegacyTranscriptWorkflowBackupManifest.load(
            from: backupRoot,
            sourceGeneration: generation
        )
        var jobs = try jobStore.legacyTranscriptJobs()
        if !jobs.isEmpty, !backup.matches(jobs) {
            let discarded = facade.discardStagedLegacyTranscriptWorkflowCutover(
                sourceGeneration: generation
            )
            guard discarded.stage == .notStarted else {
                throw LegacyTranscriptWorkflowCutoverError.blocked(
                    discarded.failure?.diagnosticCode ?? "transcript_workflow_discard_blocked"
                )
            }
            let refreshed = try LegacyTranscriptWorkflowSnapshot.capture(
                facade: facade,
                state: state,
                jobStore: jobStore
            )
            projection = try stage(refreshed, facade: facade, backupRoot: backupRoot)
            guard let refreshedGeneration = projection.sourceGeneration else {
                throw LegacyTranscriptWorkflowCutoverError.missingSourceGeneration
            }
            generation = refreshedGeneration
            backup = refreshed.backup
            jobs = try jobStore.legacyTranscriptJobs()
        }
        guard backup.matches(jobs) || (jobs.isEmpty && projection.stage == .verified) else {
            throw LegacyTranscriptWorkflowCutoverError.sourceChanged
        }

        if projection.stage == .staged {
            guard !jobs.isEmpty || backup.rows.isEmpty else {
                throw LegacyTranscriptWorkflowCutoverError.sourceChanged
            }
            projection = facade.verifyLegacyTranscriptWorkflowCutover(
                sourceGeneration: generation
            )
            guard projection.stage == .verified,
                  projection.sourceGeneration == generation,
                  projection.sourceFingerprint?.stableString == backup.sourceFingerprint else {
                throw LegacyTranscriptWorkflowCutoverError.blocked(
                    projection.failure?.diagnosticCode ?? "transcript_workflow_verify_blocked"
                )
            }
        }

        if !jobs.isEmpty {
            guard try jobStore.removeLegacyTranscriptJobs(matching: jobs) else {
                throw LegacyTranscriptWorkflowCutoverError.sourceChanged
            }
        }
        guard try jobStore.legacyTranscriptJobsAreRetired() else {
            throw LegacyTranscriptWorkflowCutoverError.legacyRowsRemain
        }
        projection = facade.commitLegacyTranscriptWorkflowCutover(
            sourceGeneration: generation
        )
        guard projection.stage == .authoritative,
              projection.sourceGeneration == generation else {
            throw LegacyTranscriptWorkflowCutoverError.commitFailed
        }
    }

    private static func stage(
        _ snapshot: LegacyTranscriptWorkflowSnapshot,
        facade: Pod0Facade,
        backupRoot: URL
    ) throws -> LegacyTranscriptWorkflowCutoverProjection {
        let evidence = try snapshot.backup.publish(to: backupRoot)
        let projection = facade.stageLegacyTranscriptWorkflowCutover(
            backupDigest: evidence.digest,
            backupByteCount: evidence.byteCount,
            rows: try snapshot.backup.coreRows(),
            candidates: snapshot.candidates
        )
        guard projection.stage == .staged,
              projection.sourceGeneration == snapshot.sourceGeneration,
              projection.sourceFingerprint == snapshot.sourceFingerprint,
              Int(projection.rowCount) == snapshot.backup.rows.count,
              Int(projection.adoptedWorkflowCount) == snapshot.candidates.count else {
            throw LegacyTranscriptWorkflowCutoverError.blocked(
                projection.failure?.diagnosticCode ?? "transcript_workflow_stage_blocked"
            )
        }
        return projection
    }
}
