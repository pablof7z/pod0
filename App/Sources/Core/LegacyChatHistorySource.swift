import CryptoKit
import Foundation

enum LegacyChatHistorySourceError: Error {
    case unavailable
    case corrupt
    case sourceChanged
}

/// Decode-only access to the retired Swift chat file.
///
/// New conversations are already Rust-owned. This source exists solely long
/// enough to stage old user data and is deleted before Rust authority commits.
final class LegacyChatHistorySource {
    static let filename = "chat_history.json"

    let fileURL: URL
    private(set) var conversations: [ChatConversation]

    convenience init(fileManager: FileManager = .default) throws {
        guard let documents = fileManager.urls(
            for: .documentDirectory,
            in: .userDomainMask
        ).first else {
            throw LegacyChatHistorySourceError.unavailable
        }
        try self.init(fileURL: documents.appendingPathComponent(Self.filename))
    }

    init(fileURL: URL) throws {
        self.fileURL = fileURL
        conversations = try Self.load(fileURL)
    }

    func retire(matching expected: [ChatConversation]) throws {
        guard FileManager.default.fileExists(atPath: fileURL.path) else {
            conversations = []
            return
        }
        guard conversations == expected else {
            throw LegacyChatHistorySourceError.sourceChanged
        }
        try FileManager.default.removeItem(at: fileURL)
        conversations = []
    }

    var isRetired: Bool {
        !FileManager.default.fileExists(atPath: fileURL.path)
    }
}

private extension LegacyChatHistorySource {
    static func load(_ fileURL: URL) throws -> [ChatConversation] {
        guard FileManager.default.fileExists(atPath: fileURL.path) else { return [] }
        let data = try Data(contentsOf: fileURL)
        if let conversations = try? decoder.decode([ChatConversation].self, from: data) {
            return conversations.sorted { $0.updatedAt > $1.updatedAt }
        }
        if let snapshot = try? decoder.decode(LegacySnapshot.self, from: data) {
            return wrap(
                messages: snapshot.messages,
                isUpgraded: snapshot.isUpgraded,
                source: data
            )
        }
        if let messages = try? decoder.decode([ChatMessage].self, from: data) {
            return wrap(messages: messages, isUpgraded: false, source: data)
        }
        throw LegacyChatHistorySourceError.corrupt
    }

    static func wrap(
        messages: [ChatMessage],
        isUpgraded: Bool,
        source: Data
    ) -> [ChatConversation] {
        guard !messages.isEmpty else { return [] }
        let stamp = messages.last?.timestamp ?? Date(timeIntervalSince1970: 0)
        return [ChatConversation(
            id: deterministicID(source),
            title: "",
            messages: messages,
            isUpgraded: isUpgraded,
            createdAt: messages.first?.timestamp ?? stamp,
            updatedAt: stamp
        )]
    }

    static func deterministicID(_ data: Data) -> UUID {
        let bytes = Array(SHA256.hash(data: data).prefix(16))
        return UUID(uuid: (
            bytes[0], bytes[1], bytes[2], bytes[3],
            bytes[4], bytes[5], bytes[6], bytes[7],
            bytes[8], bytes[9], bytes[10], bytes[11],
            bytes[12], bytes[13], bytes[14], bytes[15]
        ))
    }

    static let decoder: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return decoder
    }()

    struct LegacySnapshot: Codable {
        let messages: [ChatMessage]
        let isUpgraded: Bool
    }
}
