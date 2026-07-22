import Foundation
import Pod0Core

enum LegacyChapterWorkflowBackupError: Error, Equatable {
    case sourceChanged
    case backupMissing
    case backupConflict
    case invalidBackup
    case durabilityFailed
}

enum LegacyModelChapterWorkflowBackupClassification: String, Codable, Sendable {
    case succeededReceiptCandidate
    case ambiguousSubmission
    case blockedWithoutSubmission
    case blockedAfterPossibleSubmission
    case failedWithoutSubmission
    case failedAfterPossibleSubmission
    case cancelledWithoutSubmission
    case cancelledAfterPossibleSubmission
    case pendingUnattempted
    case obsoleteUnattempted

    static func classify(
        job: LegacyChapterWorkflowJob,
        candidate: LegacyModelChapterCutoverCandidate?
    ) -> Self {
        guard let candidate else {
            return job.state == .obsolete ? .obsoleteUnattempted : .pendingUnattempted
        }
        switch candidate.disposition {
        case .succeeded:
            return .succeededReceiptCandidate
        case .ambiguous:
            return .ambiguousSubmission
        case .blocked(_, _, let mayHaveSubmitted):
            return mayHaveSubmitted ? .blockedAfterPossibleSubmission : .blockedWithoutSubmission
        case .failed(_, _, let mayHaveSubmitted):
            return mayHaveSubmitted ? .failedAfterPossibleSubmission : .failedWithoutSubmission
        case .cancelled(let mayHaveSubmitted):
            return mayHaveSubmitted ? .cancelledAfterPossibleSubmission : .cancelledWithoutSubmission
        }
    }
}

struct LegacyModelChapterWorkflowBackupRow: Codable, Sendable, Equatable {
    let job: LegacyChapterWorkflowJob
    let classification: LegacyModelChapterWorkflowBackupClassification
}

struct LegacyModelChapterWorkflowBackupManifest: Codable, Sendable, Equatable {
    static let currentSchemaVersion = 1

    let schemaVersion: Int
    let sourceGeneration: UInt64
    let sourceFingerprint: String
    let rows: [LegacyModelChapterWorkflowBackupRow]

    init(
        sourceGeneration: UInt64,
        sourceFingerprint: String,
        rows: [LegacyModelChapterWorkflowBackupRow]
    ) {
        schemaVersion = Self.currentSchemaVersion
        self.sourceGeneration = sourceGeneration
        self.sourceFingerprint = sourceFingerprint
        self.rows = rows
    }

    func publish(to root: URL) throws {
        try LegacyWorkflowBackupStorage.publish(
            self,
            to: root,
            destinationName: fileName,
            matchingPrefix: Self.prefix(sourceGeneration),
            temporaryPrefix: "model-chapter-workflows",
            validate: { try $0.validate() }
        )
    }

    static func load(
        from root: URL,
        sourceGeneration: UInt64,
        required: Bool = true
    ) throws -> Self? {
        let manifest: Self? = try LegacyWorkflowBackupStorage.load(
            from: root,
            matchingPrefix: prefix(sourceGeneration),
            required: required,
            validate: { try $0.validate() }
        )
        guard manifest?.sourceGeneration == sourceGeneration || manifest == nil else {
            throw LegacyChapterWorkflowBackupError.invalidBackup
        }
        return manifest
    }

    func matches(_ jobs: [LegacyChapterWorkflowJob]) -> Bool {
        rows.map(\.job) == jobs.sorted { $0.id.uuidString < $1.id.uuidString }
    }

    func validate() throws {
        let jobs = rows.map(\.job)
        let ids = Set(jobs.map(\.id))
        let fingerprint: String
        do {
            fingerprint = try LegacyModelChapterWorkflowSnapshot.sourceFingerprint(for: jobs)
        } catch {
            throw LegacyChapterWorkflowBackupError.invalidBackup
        }
        guard schemaVersion == Self.currentSchemaVersion,
              sourceGeneration > 0,
              sourceFingerprint.count == 64,
              ids.count == rows.count,
              jobs.allSatisfy({ $0.kind == .chapterArtifacts }),
              fingerprint == sourceFingerprint,
              LegacyModelChapterWorkflowSnapshot.sourceGeneration(for: sourceFingerprint)
                == sourceGeneration,
              rows.allSatisfy({ row in
                  row.classification == .classify(
                      job: row.job,
                      candidate: LegacyModelChapterWorkflowSnapshot.candidate(row.job)
                  )
              }) else {
            throw LegacyChapterWorkflowBackupError.invalidBackup
        }
    }

    private var fileName: String {
        "model-chapter-workflows-v1-\(sourceGeneration)-\(sourceFingerprint).json"
    }

    private static func prefix(_ sourceGeneration: UInt64) -> String {
        "model-chapter-workflows-v1-\(sourceGeneration)-"
    }
}
