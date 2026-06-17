import Foundation
import Observation
import os.log

@MainActor
@Observable
final class ChatHistoryStore {
    private struct RustChatHistoryResponse: Decodable {
        let conversations: [ChatConversation]
        let error: String?
    }

    private let logger = Logger.app("ChatHistoryStore")

    static let shared = ChatHistoryStore()

    private static let filename = "chat_history.json"

    private let fileURL: URL?

    /// All known conversations, sorted by `updatedAt` descending.
    private(set) var conversations: [ChatConversation] = []

    private static let encoder: JSONEncoder = {
        let e = JSONEncoder()
        e.dateEncodingStrategy = .iso8601
        e.outputFormatting = [.sortedKeys]
        return e
    }()

    private static let decoder: JSONDecoder = {
        let d = JSONDecoder()
        d.dateDecodingStrategy = .iso8601
        return d
    }()

    init(fileManager: FileManager = .default) {
        if let docs = fileManager.urls(for: .documentDirectory, in: .userDomainMask).first {
            self.fileURL = docs.appendingPathComponent(Self.filename)
        } else {
            self.fileURL = nil
        }
        load()
    }

    var mostRecent: ChatConversation? {
        conversations.first(where: { !$0.isScheduledTask })
    }

    func conversation(id: UUID) -> ChatConversation? {
        conversations.first(where: { $0.id == id })
    }

    /// Inserts or updates a conversation, then re-sorts and trims to caps.
    /// Empty conversations (no messages) are ignored so transient sessions
    /// don't pollute the history list.
    func upsert(_ conversation: ChatConversation) {
        guard let normalized = rustChatHistory(op: "chat_history_upsert", conversation: conversation)
        else { return }
        conversations = normalized
        save()
    }

    /// Updates the title of a conversation in place. No-op if the id is gone.
    func setTitle(_ title: String, for id: UUID) {
        guard let idx = conversations.firstIndex(where: { $0.id == id }) else { return }
        conversations[idx].title = title
        save()
    }

    func delete(_ id: UUID) {
        conversations.removeAll { $0.id == id }
        save()
    }

    func deleteAll() {
        conversations.removeAll()
        save()
    }

    // MARK: - Persistence

    private func load() {
        guard let fileURL, FileManager.default.fileExists(atPath: fileURL.path) else {
            conversations = []
            return
        }
        do {
            let data = try Data(contentsOf: fileURL)
            // New format: bare array of ChatConversation
            if let v = try? Self.decoder.decode([ChatConversation].self, from: data) {
                conversations = rustChatHistory(op: "chat_history_normalize", conversations: v)
                    ?? []
                return
            }
            // Legacy snapshot envelope: `{ messages, isUpgraded }`
            if let snap = try? Self.decoder.decode(LegacySnapshot.self, from: data) {
                conversations = rustWrapLegacy(messages: snap.messages, isUpgraded: snap.isUpgraded)
                    ?? []
                save()
                return
            }
            // Oldest format: bare `[ChatMessage]`
            if let legacy = try? Self.decoder.decode([ChatMessage].self, from: data) {
                conversations = rustWrapLegacy(messages: legacy, isUpgraded: false)
                    ?? []
                save()
                return
            }
            logger.error("ChatHistoryStore.load: file present but no decoder matched — starting fresh")
            conversations = []
        } catch {
            logger.error("ChatHistoryStore.load failed: \(error, privacy: .public) — starting with empty history")
            conversations = []
        }
    }

    private func save() {
        guard let fileURL else { return }
        do {
            let data = try Self.encoder.encode(conversations)
            try data.write(to: fileURL, options: [.atomic])
        } catch {
            logger.error("ChatHistoryStore.save failed: \(error, privacy: .public)")
        }
    }

    /// Decoded shape of the previous single-snapshot file. Kept private here
    /// because nothing outside the store needs it after migration.
    private struct LegacySnapshot: Codable {
        var messages: [ChatMessage]
        var isUpgraded: Bool
    }

    private func rustChatHistory(
        op: String,
        conversations: [ChatConversation]? = nil,
        conversation: ChatConversation? = nil
    ) -> [ChatConversation]? {
        var payload: [String: Any] = ["op": op]
        payload["conversations"] = conversationsObject(conversations ?? self.conversations)
        if let conversation {
            payload["conversation"] = conversationObject(conversation)
        }
        return rustChatHistory(payload: payload)
    }

    private func rustWrapLegacy(messages: [ChatMessage], isUpgraded: Bool) -> [ChatConversation]? {
        rustChatHistory(payload: [
            "op": "chat_history_wrap_legacy",
            "messages": messagesObject(messages),
            "is_upgraded": isUpgraded,
        ])
    }

    private func rustChatHistory(payload: [String: Any]) -> [ChatConversation]? {
        guard let data = try? JSONSerialization.data(withJSONObject: payload),
              let json = String(data: data, encoding: .utf8)
        else { return nil }
        return json.withCString { ptr -> [ChatConversation]? in
            guard let result = nmp_app_podcast_agent_action_policy(ptr) else {
                return nil
            }
            defer { nmp_free_string(result) }
            let envelope = String(cString: result)
            guard let data = envelope.data(using: .utf8),
                  let decoded = try? Self.decoder.decode(RustChatHistoryResponse.self, from: data),
                  decoded.error == nil
            else { return nil }
            return decoded.conversations
        }
    }

    private func conversationsObject(_ conversations: [ChatConversation]) -> [[String: Any]] {
        guard let data = try? Self.encoder.encode(conversations),
              let object = try? JSONSerialization.jsonObject(with: data) as? [[String: Any]]
        else { return [] }
        return object
    }

    private func conversationObject(_ conversation: ChatConversation) -> [String: Any] {
        guard let data = try? Self.encoder.encode(conversation),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else { return [:] }
        return object
    }

    private func messagesObject(_ messages: [ChatMessage]) -> [[String: Any]] {
        guard let data = try? Self.encoder.encode(messages),
              let object = try? JSONSerialization.jsonObject(with: data) as? [[String: Any]]
        else { return [] }
        return object
    }
}
