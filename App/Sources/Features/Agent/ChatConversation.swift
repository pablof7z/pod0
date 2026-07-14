import Foundation

/// One named conversation tracked by `ChatHistoryStore`. Each chat thread the
/// user starts is persisted as a separate `ChatConversation` so they can pick
/// up an older thread from the history sheet without losing context.
///
/// `title` is generated asynchronously by `AgentChatTitleGenerator` after the
/// first assistant text reply lands; the empty string means "not yet
/// generated" and the history UI falls back to a snippet of the first user
/// message in that case.
struct ChatConversation: Identifiable, Codable, Equatable, Sendable {
    let id: UUID
    var title: String
    var messages: [ChatMessage]
    var isUpgraded: Bool
    /// Skill ids the agent has opted into for this conversation via
    /// `use_skill`. Each enabled skill contributes its tool schemas to the
    /// LLM request and its manual is part of the conversation history.
    /// Defaults to empty so every new conversation starts lean.
    var enabledSkills: Set<String>
    /// True for conversations started by `AgentScheduledTaskRunner`. Excluded
    /// from `ChatHistoryStore.mostRecent` so a scheduled run doesn't hijack
    /// the auto-resume path when the user opens the chat sheet.
    var isScheduledTask: Bool
    let createdAt: Date
    var updatedAt: Date

    init(
        id: UUID = UUID(),
        title: String = "",
        messages: [ChatMessage] = [],
        isUpgraded: Bool = false,
        enabledSkills: Set<String> = [],
        isScheduledTask: Bool = false,
        createdAt: Date = Date(),
        updatedAt: Date = Date()
    ) {
        self.id = id
        self.title = title
        self.messages = messages
        self.isUpgraded = isUpgraded
        self.enabledSkills = enabledSkills
        self.isScheduledTask = isScheduledTask
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    private enum CodingKeys: String, CodingKey {
        case id, title, messages, isUpgraded, enabledSkills, isScheduledTask, createdAt, updatedAt
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(UUID.self, forKey: .id)
        title = try c.decode(String.self, forKey: .title)
        messages = try c.decode([ChatMessage].self, forKey: .messages)
        isUpgraded = try c.decode(Bool.self, forKey: .isUpgraded)
        // Forward-compat: old persisted snapshots predate these fields.
        enabledSkills = try c.decodeIfPresent(Set<String>.self, forKey: .enabledSkills) ?? []
        isScheduledTask = try c.decodeIfPresent(Bool.self, forKey: .isScheduledTask) ?? false
        createdAt = try c.decode(Date.self, forKey: .createdAt)
        updatedAt = try c.decode(Date.self, forKey: .updatedAt)
    }

    /// First user message text — used in the history list when `title` is empty.
    var firstUserSnippet: String {
        for msg in messages {
            if case .user = msg.role { return msg.text }
        }
        return ""
    }
}
