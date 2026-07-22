import CryptoKit
import Foundation
import Pod0Core

enum LegacyTranscriptWorkflowBackupClassification: String, Codable, Sendable {
    case restart
    case recoverProvider = "recover_provider"
    case ambiguous
    case blocked
    case failed
    case cancelled
    case succeeded
    case indexPending = "index_pending"
    case indexSucceeded = "index_succeeded"
    case obsolete

    var coreValue: LegacyTranscriptWorkflowRowClassification {
        switch self {
        case .restart: .restart
        case .recoverProvider: .recoverProvider
        case .ambiguous: .ambiguous
        case .blocked: .blocked
        case .failed: .failed
        case .cancelled: .cancelled
        case .succeeded: .succeeded
        case .indexPending: .indexPending
        case .indexSucceeded: .indexSucceeded
        case .obsolete: .obsolete
        }
    }

    var rank: UInt8 {
        switch self {
        case .restart: 1
        case .recoverProvider: 2
        case .ambiguous: 3
        case .blocked: 4
        case .failed: 5
        case .cancelled: 6
        case .succeeded: 7
        case .indexPending: 8
        case .indexSucceeded: 9
        case .obsolete: 10
        }
    }
}

struct LegacyTranscriptWorkflowBackupRow: Codable, Sendable, Equatable {
    let job: LegacyTranscriptWorkflowJob
    let classification: LegacyTranscriptWorkflowBackupClassification

    func coreValue() throws -> Pod0Core.LegacyTranscriptWorkflowBackupRow {
        let bytes = try LegacyWorkflowBackupStorage.encodedData(job)
        let fingerprint = ArtifactRepository.hash(bytes)
        guard let digest = ContentDigest(hexadecimal: fingerprint) else {
            throw LegacyChapterWorkflowBackupError.invalidBackup
        }
        return Pod0Core.LegacyTranscriptWorkflowBackupRow(
            episodeId: EpisodeId(uuid: job.subjectID),
            rowBytes: bytes,
            rowFingerprint: digest,
            classification: classification.coreValue
        )
    }
}

struct LegacyTranscriptWorkflowBackupManifest: Codable, Sendable, Equatable {
    static let currentSchemaVersion = 1

    struct Evidence: Equatable {
        let digest: ContentDigest
        let byteCount: UInt64
    }

    let schemaVersion: Int
    let sourceGeneration: UInt64
    let sourceFingerprint: String
    let rows: [LegacyTranscriptWorkflowBackupRow]

    init(sourceGeneration: UInt64, sourceFingerprint: String, rows: [LegacyTranscriptWorkflowBackupRow]) {
        schemaVersion = Self.currentSchemaVersion
        self.sourceGeneration = sourceGeneration
        self.sourceFingerprint = sourceFingerprint
        self.rows = rows
    }

    func publish(to root: URL) throws -> Evidence {
        try LegacyWorkflowBackupStorage.publish(
            self,
            to: root,
            destinationName: fileName,
            matchingPrefix: Self.prefix(sourceGeneration),
            temporaryPrefix: "transcript-workflows",
            validate: { try $0.validate() }
        )
        return try evidence(at: root)
    }

    func evidence(at root: URL) throws -> Evidence {
        let data = try Data(contentsOf: root.appendingPathComponent(fileName))
        guard let digest = ContentDigest(hexadecimal: ArtifactRepository.hash(data)) else {
            throw LegacyChapterWorkflowBackupError.invalidBackup
        }
        return Evidence(digest: digest, byteCount: UInt64(data.count))
    }

    static func load(from root: URL, sourceGeneration: UInt64) throws -> Self {
        guard let manifest: Self = try LegacyWorkflowBackupStorage.load(
            from: root,
            matchingPrefix: prefix(sourceGeneration),
            validate: { try $0.validate() }
        ) else { throw LegacyChapterWorkflowBackupError.backupMissing }
        guard manifest.sourceGeneration == sourceGeneration else {
            throw LegacyChapterWorkflowBackupError.invalidBackup
        }
        return manifest
    }

    func coreRows() throws -> [Pod0Core.LegacyTranscriptWorkflowBackupRow] {
        Self.sorted(try rows.map { try $0.coreValue() })
    }

    func matches(_ jobs: [LegacyTranscriptWorkflowJob]) -> Bool {
        rows.map(\.job) == jobs.sorted { $0.id.uuidString < $1.id.uuidString }
    }

    func validate() throws {
        let jobs = rows.map(\.job)
        guard schemaVersion == Self.currentSchemaVersion,
              sourceGeneration > 0,
              sourceFingerprint.count == 64,
              Set(jobs.map(\.id)).count == jobs.count,
              jobs.allSatisfy({ LegacyTranscriptWorkflowJobKind.allCases.contains($0.kind) })
        else { throw LegacyChapterWorkflowBackupError.invalidBackup }
        let coreRows = try coreRows()
        let fingerprint = Self.sourceFingerprint(for: coreRows)
        guard fingerprint.stableString == sourceFingerprint,
              Self.sourceGeneration(for: fingerprint) == sourceGeneration else {
            throw LegacyChapterWorkflowBackupError.invalidBackup
        }
    }

    static func sourceFingerprint(
        for rows: [Pod0Core.LegacyTranscriptWorkflowBackupRow]
    ) -> ContentDigest {
        var hasher = SHA256()
        hasher.update(data: Data("pod0-legacy-transcript-workflow-source-v1".utf8))
        for (ordinal, row) in sorted(rows).enumerated() {
            hasher.update(data: bigEndian(UInt64(ordinal)))
            hasher.update(data: bigEndian(row.episodeId.high))
            hasher.update(data: bigEndian(row.episodeId.low))
            hasher.update(data: digestBytes(row.rowFingerprint))
            hasher.update(data: Data(classificationWire(row.classification).utf8))
        }
        let hexadecimal = hasher.finalize().map { String(format: "%02x", $0) }.joined()
        return ContentDigest(hexadecimal: hexadecimal)!
    }

    static func sourceGeneration(for digest: ContentDigest) -> UInt64 {
        max(1, digest.word0 & UInt64(Int64.max))
    }

    private var fileName: String {
        "transcript-workflows-v1-\(sourceGeneration)-\(sourceFingerprint).json"
    }

    private static func prefix(_ generation: UInt64) -> String {
        "transcript-workflows-v1-\(generation)-"
    }
}

private extension LegacyTranscriptWorkflowBackupManifest {
    static func sorted(
        _ rows: [Pod0Core.LegacyTranscriptWorkflowBackupRow]
    ) -> [Pod0Core.LegacyTranscriptWorkflowBackupRow] {
        rows.sorted { lhs, rhs in
            if lhs.episodeId.high != rhs.episodeId.high { return lhs.episodeId.high < rhs.episodeId.high }
            if lhs.episodeId.low != rhs.episodeId.low { return lhs.episodeId.low < rhs.episodeId.low }
            if lhs.rowFingerprint.stableString != rhs.rowFingerprint.stableString {
                return lhs.rowFingerprint.stableString < rhs.rowFingerprint.stableString
            }
            return classificationRank(lhs.classification) < classificationRank(rhs.classification)
        }
    }

    static func bigEndian(_ value: UInt64) -> Data {
        withUnsafeBytes(of: value.bigEndian) { Data($0) }
    }

    static func digestBytes(_ value: ContentDigest) -> Data {
        bigEndian(value.word0) + bigEndian(value.word1)
            + bigEndian(value.word2) + bigEndian(value.word3)
    }

    static func classificationWire(_ value: LegacyTranscriptWorkflowRowClassification) -> String {
        switch value {
        case .restart: "restart"
        case .recoverProvider: "recover_provider"
        case .ambiguous: "ambiguous"
        case .blocked: "blocked"
        case .failed: "failed"
        case .cancelled: "cancelled"
        case .succeeded: "succeeded"
        case .indexPending: "index_pending"
        case .indexSucceeded: "index_succeeded"
        case .obsolete: "obsolete"
        }
    }

    static func classificationRank(_ value: LegacyTranscriptWorkflowRowClassification) -> UInt8 {
        LegacyTranscriptWorkflowBackupClassification(rawValue: classificationWire(value))!.rank
    }
}
