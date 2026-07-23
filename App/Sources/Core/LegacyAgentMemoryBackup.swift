import CryptoKit
import Foundation
import Pod0Core

enum LegacyAgentMemoryBackupError: Error {
    case missing
    case corrupt
    case conflict
    case evidenceMismatch
}

struct LegacyAgentMemoryBackup: Codable, Equatable {
    let formatVersion: Int
    let memories: [AgentMemory]
    let compiled: CompiledAgentMemory?
    let persistenceGeneration: UInt64

    init(state: AppState) {
        formatVersion = 1
        memories = state.agentMemories.sorted { $0.id.uuidString < $1.id.uuidString }
        compiled = state.compiledMemory
        persistenceGeneration = state.persistenceGeneration
    }

    func evidence() throws -> (digest: ContentDigest, byteCount: UInt64) {
        let data = try encoded()
        let hex = SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
        guard let digest = ContentDigest(hexadecimal: hex) else {
            throw LegacyAgentMemoryBackupError.corrupt
        }
        return (digest, UInt64(data.count))
    }

    func publish(to root: URL, sourceGeneration: UInt64) throws {
        let data = try encoded()
        let destination = Self.url(in: root, sourceGeneration: sourceGeneration)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        if FileManager.default.fileExists(atPath: destination.path) {
            guard try Data(contentsOf: destination) == data else {
                throw LegacyAgentMemoryBackupError.conflict
            }
            return
        }
        try data.write(to: destination, options: [.atomic, .completeFileProtection])
        guard try Data(contentsOf: destination) == data else {
            throw LegacyAgentMemoryBackupError.evidenceMismatch
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
            throw LegacyAgentMemoryBackupError.missing
        }
        let data = try Data(contentsOf: source)
        let backup: Self
        do { backup = try decoder.decode(Self.self, from: data) }
        catch { throw LegacyAgentMemoryBackupError.corrupt }
        guard backup.formatVersion == 1 else {
            throw LegacyAgentMemoryBackupError.corrupt
        }
        let evidence = try backup.evidence()
        guard data == (try backup.encoded()),
              expectedDigest.map({ $0 == evidence.digest }) ?? true,
              expectedByteCount.map({ $0 == evidence.byteCount }) ?? true
        else { throw LegacyAgentMemoryBackupError.evidenceMismatch }
        return backup
    }

    private func encoded() throws -> Data {
        try Self.encoder.encode(self)
    }

    private static func url(in root: URL, sourceGeneration: UInt64) -> URL {
        root.appendingPathComponent(
            "agent-memory-\(sourceGeneration)-v1.json",
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
