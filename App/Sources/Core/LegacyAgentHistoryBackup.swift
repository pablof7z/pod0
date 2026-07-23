import CryptoKit
import Foundation
import Pod0Core

enum LegacyAgentHistoryBackupError: Error {
    case missing
    case corrupt
    case conflict
    case evidenceMismatch
}

struct LegacyAgentHistoryBackup: Codable, Equatable {
    let formatVersion: Int
    let conversations: [ChatConversation]

    init(conversations: [ChatConversation]) {
        formatVersion = 1
        self.conversations = conversations.sorted { $0.id.uuidString < $1.id.uuidString }
    }

    func evidence() throws -> (digest: ContentDigest, byteCount: UInt64) {
        let data = try encoded()
        let hex = SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
        guard let digest = ContentDigest(hexadecimal: hex) else {
            throw LegacyAgentHistoryBackupError.corrupt
        }
        return (digest, UInt64(data.count))
    }

    func publish(to root: URL, sourceGeneration: UInt64) throws {
        let data = try encoded()
        let destination = Self.url(in: root, sourceGeneration: sourceGeneration)
        try FileManager.default.createDirectory(
            at: root,
            withIntermediateDirectories: true
        )
        if FileManager.default.fileExists(atPath: destination.path) {
            guard try Data(contentsOf: destination) == data else {
                throw LegacyAgentHistoryBackupError.conflict
            }
            return
        }
        try data.write(to: destination, options: [.atomic, .completeFileProtection])
        guard try Data(contentsOf: destination) == data else {
            throw LegacyAgentHistoryBackupError.evidenceMismatch
        }
    }

    static func load(
        from root: URL,
        sourceGeneration: UInt64,
        expectedDigest: ContentDigest?,
        expectedByteCount: UInt64?
    ) throws -> Self {
        let source = url(in: root, sourceGeneration: sourceGeneration)
        guard FileManager.default.fileExists(atPath: source.path) else {
            throw LegacyAgentHistoryBackupError.missing
        }
        let data = try Data(contentsOf: source)
        let backup: Self
        do { backup = try decoder.decode(Self.self, from: data) }
        catch { throw LegacyAgentHistoryBackupError.corrupt }
        guard backup.formatVersion == 1 else {
            throw LegacyAgentHistoryBackupError.corrupt
        }
        let evidence = try backup.evidence()
        guard data == (try backup.encoded()),
              expectedDigest.map({ $0 == evidence.digest }) ?? true,
              expectedByteCount.map({ $0 == evidence.byteCount }) ?? true
        else { throw LegacyAgentHistoryBackupError.evidenceMismatch }
        return backup
    }

    private func encoded() throws -> Data {
        try Self.encoder.encode(self)
    }

    private static func url(in root: URL, sourceGeneration: UInt64) -> URL {
        root.appendingPathComponent(
            "agent-history-\(sourceGeneration)-v1.json",
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
