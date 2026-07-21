import Darwin
import Foundation
import Pod0Core

enum LegacyModelChapterWorkflowBackupError: Error, Equatable {
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
        job: WorkJob,
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
    let job: WorkJob
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
        try validate()
        let destination = fileURL(in: root)
        if let existing = try Self.load(
            from: root,
            sourceGeneration: sourceGeneration,
            required: false
        ) {
            guard existing == self else {
                throw LegacyModelChapterWorkflowBackupError.backupConflict
            }
            try Self.synchronizePublishedFile(at: destination, in: root)
            return
        }
        try FileManager.default.createDirectory(
            at: root,
            withIntermediateDirectories: true
        )
        let data = try Self.encodedData(LegacyModelChapterWorkflowBackupEnvelope(
            manifest: self,
            integrityDigest: ArtifactRepository.hash(try Self.encodedData(self))
        ))
        let temporary = root.appendingPathComponent(
            ".model-chapter-workflows-\(UUID().uuidString).tmp",
            isDirectory: false
        )
        defer { try? FileManager.default.removeItem(at: temporary) }
        try data.write(to: temporary, options: .atomic)
        do {
            try FileManager.default.linkItem(
                at: temporary,
                to: destination
            )
        } catch {
            if let existing = try? Self.load(
                from: root,
                sourceGeneration: sourceGeneration
            ), existing == self {
                try Self.synchronizePublishedFile(at: destination, in: root)
                return
            }
            throw LegacyModelChapterWorkflowBackupError.backupConflict
        }
        try Self.synchronizePublishedFile(at: destination, in: root)
        guard try Self.load(from: root, sourceGeneration: sourceGeneration) == self else {
            throw LegacyModelChapterWorkflowBackupError.invalidBackup
        }
    }

    static func load(
        from root: URL,
        sourceGeneration: UInt64,
        required: Bool = true
    ) throws -> Self? {
        let prefix = "model-chapter-workflows-v1-\(sourceGeneration)-"
        let files = (try? FileManager.default.contentsOfDirectory(
            at: root,
            includingPropertiesForKeys: nil
        ))?.filter {
            $0.lastPathComponent.hasPrefix(prefix) && $0.pathExtension == "json"
        } ?? []
        guard files.count <= 1 else {
            throw LegacyModelChapterWorkflowBackupError.backupConflict
        }
        guard let file = files.first else {
            if required { throw LegacyModelChapterWorkflowBackupError.backupMissing }
            return nil
        }
        let envelope: LegacyModelChapterWorkflowBackupEnvelope
        do {
            envelope = try JSONDecoder().decode(
                LegacyModelChapterWorkflowBackupEnvelope.self,
                from: Data(contentsOf: file)
            )
        } catch {
            throw LegacyModelChapterWorkflowBackupError.invalidBackup
        }
        let manifestData = try Self.encodedData(envelope.manifest)
        guard ArtifactRepository.hash(manifestData) == envelope.integrityDigest,
              envelope.manifest.sourceGeneration == sourceGeneration else {
            throw LegacyModelChapterWorkflowBackupError.invalidBackup
        }
        try envelope.manifest.validate()
        return envelope.manifest
    }

    func matches(_ jobs: [WorkJob]) -> Bool {
        rows.map(\.job) == jobs.sorted { $0.id.uuidString < $1.id.uuidString }
    }

    private func validate() throws {
        let jobs = rows.map(\.job)
        let ids = Set(jobs.map(\.id))
        let fingerprint: String
        do {
            fingerprint = try LegacyModelChapterWorkflowSnapshot.sourceFingerprint(for: jobs)
        } catch {
            throw LegacyModelChapterWorkflowBackupError.invalidBackup
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
            throw LegacyModelChapterWorkflowBackupError.invalidBackup
        }
    }

    private func fileURL(in root: URL) -> URL {
        root.appendingPathComponent(
            "model-chapter-workflows-v1-\(sourceGeneration)-\(sourceFingerprint).json",
            isDirectory: false
        )
    }

    private static func encodedData<T: Encodable>(_ value: T) throws -> Data {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        return try encoder.encode(value)
    }

    /// Flushes both the linked file and its directory entry before the legacy
    /// SQLite transaction is allowed to delete source rows.
    private static func synchronizePublishedFile(at file: URL, in root: URL) throws {
        try synchronize(file, requestFullSync: true)
        try synchronize(root, requestFullSync: false)
    }

    private static func synchronize(_ url: URL, requestFullSync: Bool) throws {
        let descriptor = Darwin.open(url.path, O_RDONLY)
        guard descriptor >= 0 else {
            throw LegacyModelChapterWorkflowBackupError.durabilityFailed
        }
        defer { _ = Darwin.close(descriptor) }
        if requestFullSync, Darwin.fcntl(descriptor, F_FULLFSYNC) == 0 { return }
        guard Darwin.fsync(descriptor) == 0 else {
            throw LegacyModelChapterWorkflowBackupError.durabilityFailed
        }
    }
}

private struct LegacyModelChapterWorkflowBackupEnvelope: Codable {
    let manifest: LegacyModelChapterWorkflowBackupManifest
    let integrityDigest: String
}
