import CryptoKit
import Foundation

enum LegacyDownloadWorkflowBackupError: Error, Equatable {
    case sourceChanged
    case backupMissing
    case backupCorrupt
}

struct LegacyDownloadWorkflowBackup: Codable, Equatable {
    struct TaskEvidence: Codable, Equatable {
        let taskIdentifier: Int
        let jobID: UUID?
        let episodeID: UUID?
        let inputVersion: String?
        let originalURL: String?
        let state: Int
        let receivedByteCount: Int64
        let expectedByteCount: Int64
    }

    struct CandidateEvidence: Codable, Equatable {
        enum Disposition: String, Codable {
            case available
            case restart
        }

        let episodeID: UUID
        let origin: LegacyDownloadIntentOrigin
        let disposition: Disposition
        let sourcePath: String?
        let byteCount: Int64?
        let resumeByteCount: Int?
        let resumeDigest: String?
    }

    let formatVersion: Int
    let sourceGeneration: UInt64
    let persistenceGeneration: UInt64
    let jobs: [WorkJob]
    let artifacts: [ArtifactRecord]
    let tasks: [TaskEvidence]
    let candidates: [CandidateEvidence]

    func publish(to url: URL) throws {
        let data = try Self.encoder.encode(self)
        try FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        if FileManager.default.fileExists(atPath: url.path) {
            guard try Data(contentsOf: url) == data else {
                throw LegacyDownloadWorkflowBackupError.sourceChanged
            }
            return
        }
        try data.write(to: url, options: [.atomic, .completeFileProtection])
    }

    static func load(from url: URL) throws -> Self {
        guard FileManager.default.fileExists(atPath: url.path) else {
            throw LegacyDownloadWorkflowBackupError.backupMissing
        }
        do {
            return try decoder.decode(Self.self, from: Data(contentsOf: url))
        } catch {
            throw LegacyDownloadWorkflowBackupError.backupCorrupt
        }
    }

    static func generation(
        persistenceGeneration: UInt64,
        jobs: [WorkJob],
        artifacts: [ArtifactRecord],
        tasks: [TaskEvidence],
        candidates: [CandidateEvidence]
    ) throws -> UInt64 {
        let seed = GenerationSeed(
            persistenceGeneration: persistenceGeneration,
            jobs: jobs,
            artifacts: artifacts,
            tasks: tasks,
            candidates: candidates
        )
        let digest = SHA256.hash(data: try encoder.encode(seed))
        let value = digest.prefix(8).reduce(UInt64.zero) { ($0 << 8) | UInt64($1) }
        return value == 0 ? 1 : value
    }

    static func digest(_ data: Data?) -> String? {
        data.map { SHA256.hash(data: $0).map { String(format: "%02x", $0) }.joined() }
    }

    private struct GenerationSeed: Codable {
        let persistenceGeneration: UInt64
        let jobs: [WorkJob]
        let artifacts: [ArtifactRecord]
        let tasks: [TaskEvidence]
        let candidates: [CandidateEvidence]
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
}
