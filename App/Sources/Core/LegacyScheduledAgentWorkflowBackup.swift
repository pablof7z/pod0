import CryptoKit
import Foundation
import Pod0Core

enum LegacyScheduledAgentWorkflowBackupError: Error, Equatable {
    case backupMissing
    case backupCorrupt
    case backupConflict
    case evidenceMismatch
}

struct LegacyScheduledAgentArtifactRow: Codable, Equatable, Sendable {
    let record: ArtifactRecord
    let selected: Bool
}

/// Immutable rollback evidence for the one-shot Swift scheduled-agent cutover.
/// The file contains every legacy authority component, including conversation
/// output needed to requalify completed occurrences inside Rust.
struct LegacyScheduledAgentWorkflowBackup: Codable, Equatable, Sendable {
    let formatVersion: Int
    let persistenceGeneration: UInt64
    let defaultModelReference: String
    let tasks: [AgentScheduledTask]
    let jobs: [WorkJob]
    let artifacts: [LegacyScheduledAgentArtifactRow]
    let conversations: [ChatConversation]

    func encoded() throws -> Data {
        try Self.encoder.encode(self)
    }

    func evidence() throws -> (digest: ContentDigest, byteCount: UInt64) {
        let data = try encoded()
        guard let digest = ContentDigest(
            hexadecimal: SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
        ) else { throw LegacyScheduledAgentWorkflowBackupError.backupCorrupt }
        return (digest, UInt64(data.count))
    }

    func publish(to root: URL, sourceGeneration: UInt64) throws -> URL {
        let data = try encoded()
        let destination = Self.url(in: root, sourceGeneration: sourceGeneration)
        try FileManager.default.createDirectory(
            at: root,
            withIntermediateDirectories: true
        )
        if FileManager.default.fileExists(atPath: destination.path) {
            guard try Data(contentsOf: destination) == data else {
                throw LegacyScheduledAgentWorkflowBackupError.backupConflict
            }
            return destination
        }
        try data.write(to: destination, options: [.atomic, .completeFileProtection])
        guard try Data(contentsOf: destination) == data else {
            throw LegacyScheduledAgentWorkflowBackupError.evidenceMismatch
        }
        return destination
    }

    static func load(
        from root: URL,
        sourceGeneration: UInt64,
        expectedDigest: ContentDigest?,
        expectedByteCount: UInt64?
    ) throws -> Self {
        let source = url(in: root, sourceGeneration: sourceGeneration)
        guard FileManager.default.fileExists(atPath: source.path) else {
            throw LegacyScheduledAgentWorkflowBackupError.backupMissing
        }
        let data = try Data(contentsOf: source)
        let backup: Self
        do { backup = try decoder.decode(Self.self, from: data) }
        catch { throw LegacyScheduledAgentWorkflowBackupError.backupCorrupt }
        guard backup.formatVersion == 1 else {
            throw LegacyScheduledAgentWorkflowBackupError.backupCorrupt
        }
        let evidence = try backup.evidence()
        guard data == (try backup.encoded()),
              expectedDigest.map({ $0 == evidence.digest }) ?? true,
              expectedByteCount.map({ $0 == evidence.byteCount }) ?? true
        else { throw LegacyScheduledAgentWorkflowBackupError.evidenceMismatch }
        return backup
    }

    static func url(in root: URL, sourceGeneration: UInt64) -> URL {
        root.appendingPathComponent(
            "scheduled-agent-\(sourceGeneration)-v1.json",
            isDirectory: false
        )
    }

    private static let encoder: JSONEncoder = {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .secondsSince1970
        encoder.outputFormatting = [.sortedKeys]
        return encoder
    }()

    private static let decoder: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .secondsSince1970
        return decoder
    }()
}
