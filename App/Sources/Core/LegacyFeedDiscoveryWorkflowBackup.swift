import CryptoKit
import Foundation
import Pod0Core

enum LegacyFeedDiscoveryWorkflowBackupError: Error, Equatable {
    case backupMissing
    case backupCorrupt
    case backupConflict
    case evidenceMismatch
}

/// Exact decode-only payload written by the retired Swift planner.
struct LegacyFeedDiscoveryPayload: Codable, Sendable, Equatable {
    struct EpisodeInput: Codable, Sendable, Equatable {
        let episodeID: UUID
        let inputVersion: String
        let pubDate: Date
        let title: String
    }

    let podcastID: UUID
    let occurrenceID: String
    let discoveredAt: Date
    let episodes: [EpisodeInput]
    let autoDownloadPolicy: AutoDownloadPolicy?
    let notificationsEnabled: Bool
    let policyVersion: String
}

/// Exact decode-only child payload written by the retired Swift executor.
struct LegacyNewEpisodeNotificationPayload: Codable, Sendable, Equatable {
    let discoveredAt: Date
    let podcastID: UUID
    let episodeTitle: String
}

enum LegacyFeedDiscoveryJobKind: String, Codable, Sendable {
    case feedDiscovery
    case newEpisodeNotification
}

/// Exact decode-only row shape for authority values no longer representable
/// by the mutable native `WorkJobKind`.
struct LegacyFeedDiscoveryWorkJob: Codable, Sendable, Equatable {
    let id: UUID
    let idempotencyKey: String
    let kind: LegacyFeedDiscoveryJobKind
    let subjectID: UUID
    let inputVersion: String
    let occurrenceID: String?
    let payloadVersion: Int
    let payload: Data?
    let state: WorkJobState
    let priority: Int
    let resourceClass: WorkResourceClass
    let attempt: Int
    let maxAttempts: Int
    let notBefore: Date
    let leaseToken: UUID?
    let leaseOwner: String?
    let leaseExpiresAt: Date?
    let externalProvider: String?
    let externalOperationID: String?
    let externalOperationState: String?
    let outputVersion: String?
    let lastErrorClass: JobErrorClass?
    let lastErrorMessage: String?
    let createdAt: Date
    let updatedAt: Date
}

enum LegacyFeedDiscoveryArtifactKind: String, Codable, Sendable {
    case feedDiscovery
    case notificationDelivery
}

struct LegacyFeedDiscoveryArtifactRecord: Codable, Sendable, Equatable {
    let kind: LegacyFeedDiscoveryArtifactKind
    let subjectID: UUID
    let inputVersion: String
    let outputVersion: String
    let contentHash: String
    let location: String?
    let origin: String?
    let schemaVersion: Int
    let integrity: ArtifactIntegrity
    let verifiedAt: Date
}

struct LegacyFeedDiscoveryWorkflowBackup: Codable, Equatable, Sendable {
    let formatVersion: Int
    let persistenceGeneration: UInt64
    let capturedAt: Date
    let notificationsEnabled: Bool
    let jobs: [LegacyFeedDiscoveryWorkJob]
    let artifacts: [LegacyFeedDiscoveryArtifactRecord]

    func encoded() throws -> Data {
        try Self.encoder.encode(self)
    }

    func evidence() throws -> (digest: ContentDigest, byteCount: UInt64) {
        let data = try encoded()
        let hexadecimal = SHA256.hash(data: data)
            .map { String(format: "%02x", $0) }
            .joined()
        guard let digest = ContentDigest(hexadecimal: hexadecimal) else {
            throw LegacyFeedDiscoveryWorkflowBackupError.backupCorrupt
        }
        return (digest, UInt64(data.count))
    }

    func publish(to root: URL, sourceGeneration: UInt64) throws -> URL {
        let data = try encoded()
        let destination = Self.url(in: root, sourceGeneration: sourceGeneration)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        if FileManager.default.fileExists(atPath: destination.path) {
            guard try Data(contentsOf: destination) == data else {
                throw LegacyFeedDiscoveryWorkflowBackupError.backupConflict
            }
            return destination
        }
        try data.write(to: destination, options: [.atomic, .completeFileProtection])
        guard try Data(contentsOf: destination) == data else {
            throw LegacyFeedDiscoveryWorkflowBackupError.evidenceMismatch
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
            throw LegacyFeedDiscoveryWorkflowBackupError.backupMissing
        }
        let data = try Data(contentsOf: source)
        let backup: Self
        do { backup = try decoder.decode(Self.self, from: data) }
        catch { throw LegacyFeedDiscoveryWorkflowBackupError.backupCorrupt }
        guard backup.formatVersion == 1 else {
            throw LegacyFeedDiscoveryWorkflowBackupError.backupCorrupt
        }
        let evidence = try backup.evidence()
        guard data == (try backup.encoded()),
              expectedDigest.map({ $0 == evidence.digest }) ?? true,
              expectedByteCount.map({ $0 == evidence.byteCount }) ?? true
        else { throw LegacyFeedDiscoveryWorkflowBackupError.evidenceMismatch }
        return backup
    }

    static func url(in root: URL, sourceGeneration: UInt64) -> URL {
        root.appendingPathComponent(
            "feed-discovery-\(sourceGeneration)-v1.json",
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
