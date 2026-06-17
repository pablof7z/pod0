import Foundation

// MARK: - Conversation history tools
//
// Skill-gated by `conversation_history`. Swift marshals raw local history
// facts; Rust owns source normalization, caps, ordering, lexical search,
// snippets, display fallbacks, and result row shape.

extension AgentTools {

    // MARK: - Dispatcher

    @MainActor
    static func dispatchConversations(
        name: String,
        args: [String: Any],
        store: AppStateStore
    ) async -> String {
        guard name == Names.listConversations || name == Names.searchConversations else {
            return toolError("Unknown conversation tool: \(name)")
        }
        guard let envelope = store.kernel?.agentConversationHistoryEnvelope(
            request: conversationHistoryRequest(op: name, args: args, store: store)
        ) else {
            return toolError("Conversation history is unavailable")
        }
        return envelope
    }

    @MainActor
    private static func conversationHistoryRequest(
        op: String,
        args: [String: Any],
        store: AppStateStore
    ) -> [String: Any] {
        [
            "op": op,
            "args": args,
            "in_app": ChatHistoryStore.shared.conversations.map(serializeRawInAppConversation),
            "nostr": store.state.nostrConversations.map(serializeRawNostrConversation),
            "friends": store.state.friends.map {
                [
                    "identifier": $0.identifier,
                    "display_name": $0.displayName,
                ]
            },
        ]
    }

    private static func serializeRawInAppConversation(_ conversation: ChatConversation) -> [String: Any] {
        [
            "id": conversation.id.uuidString,
            "title": conversation.title,
            "updated_at": Int(conversation.updatedAt.timeIntervalSince1970),
            "messages": conversation.messages.map {
                [
                    "role": roleString($0.role),
                    "text": $0.text,
                    "timestamp": Int($0.timestamp.timeIntervalSince1970),
                ]
            },
        ]
    }

    private static func serializeRawNostrConversation(_ record: NostrConversationRecord) -> [String: Any] {
        [
            "root_event_id": record.rootEventID,
            "counterparty_pubkey": record.counterpartyPubkey,
            "first_seen": Int(record.firstSeen.timeIntervalSince1970),
            "last_touched": Int(record.lastTouched.timeIntervalSince1970),
            "turns": record.turns.map {
                [
                    "direction": $0.direction.rawValue,
                    "content": $0.content,
                    "created_at": Int($0.createdAt.timeIntervalSince1970),
                ]
            },
        ]
    }

    private static func roleString(_ role: ChatMessage.Role) -> String {
        switch role {
        case .user: "user"
        case .assistant: "assistant"
        default: "other"
        }
    }
}
