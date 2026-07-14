import Foundation
import Observation
import os.log

@MainActor
@Observable
final class ChatHistoryStore {
    private let logger = Logger.app("ChatHistoryStore")

    static let shared = ChatHistoryStore()

    private static let filename = "chat_history.json"
    /// Per-conversation message cap — matches the previous single-conversation
    /// behaviour where the history file was truncated to the last 100.
    private static let maxMessagesPerConversation = 100
    /// Total-conversation cap. Oldest by `updatedAt` are evicted when exceeded.
    /// Picked high enough that a heavy user never bumps into it accidentally
    /// while still bounding the on-disk index file.
    private static let maxConversations = 50

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
        guard !conversation.messages.isEmpty else {
            // If an existing conversation was emptied, remove it instead.
            if let idx = conversations.firstIndex(where: { $0.id == conversation.id }) {
                conversations.remove(at: idx)
                save()
            }
            return
        }
        var convo = conversation
        if convo.messages.count > Self.maxMessagesPerConversation {
            convo.messages = Array(convo.messages.suffix(Self.maxMessagesPerConversation))
        }
        if let idx = conversations.firstIndex(where: { $0.id == convo.id }) {
            conversations[idx] = convo
        } else {
            conversations.append(convo)
        }
        conversations.sort { $0.updatedAt > $1.updatedAt }
        if conversations.count > Self.maxConversations {
            conversations = Array(conversations.prefix(Self.maxConversations))
        }
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
                conversations = v.sorted { $0.updatedAt > $1.updatedAt }
                return
            }
            // Legacy snapshot envelope: `{ messages, isUpgraded }`
            if let snap = try? Self.decoder.decode(LegacySnapshot.self, from: data) {
                conversations = Self.wrap(messages: snap.messages, isUpgraded: snap.isUpgraded)
                save()
                return
            }
            // Oldest format: bare `[ChatMessage]`
            if let legacy = try? Self.decoder.decode([ChatMessage].self, from: data) {
                conversations = Self.wrap(messages: legacy, isUpgraded: false)
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

    private static func wrap(messages: [ChatMessage], isUpgraded: Bool) -> [ChatConversation] {
        guard !messages.isEmpty else { return [] }
        let stamp = messages.last?.timestamp ?? Date()
        return [
            ChatConversation(
                id: UUID(),
                title: "",
                messages: messages,
                isUpgraded: isUpgraded,
                createdAt: messages.first?.timestamp ?? stamp,
                updatedAt: stamp
            ),
        ]
    }

    /// Decoded shape of the previous single-snapshot file. Kept private here
    /// because nothing outside the store needs it after migration.
    private struct LegacySnapshot: Codable {
        var messages: [ChatMessage]
        var isUpgraded: Bool
    }
}
