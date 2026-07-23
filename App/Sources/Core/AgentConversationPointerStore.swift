import Foundation
import Pod0Core

/// Native navigation preference only. Rust remains the durable conversation
/// authority; a stale pointer simply resolves to an empty projection.
struct AgentConversationPointerStore {
    private static let key = "pod0.agent.lastConversationID.v1"
    let defaults: UserDefaults

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
    }

    func load() -> ConversationId? {
        guard let value = defaults.string(forKey: Self.key),
              let uuid = UUID(uuidString: value) else { return nil }
        return ConversationId(uuid: uuid)
    }

    func save(_ conversationID: ConversationId?) {
        guard let uuid = conversationID?.uuid else {
            defaults.removeObject(forKey: Self.key)
            return
        }
        defaults.set(uuid.uuidString, forKey: Self.key)
    }
}
