import Foundation

// MARK: - Conversation history tools
//
// Skill-gated by `conversation_history`. Both handlers run on @MainActor so
// they can access `ChatHistoryStore.shared` (in-app threads) and the Nostr
// conversation log in `AppStateStore.state.nostrConversations` without a
// context switch.
//
// Search is lexical (case-insensitive substring) rather than embedding-based.
// In-app history is capped at 50 × 100 messages; Nostr history is similarly
// modest, so a full-text scan is fast and avoids the cost of maintaining a
// separate vector index for conversations.

extension AgentTools {

    // MARK: - Dispatcher

    @MainActor
    static func dispatchConversations(
        name: String,
        args: [String: Any],
        store: AppStateStore
    ) async -> String {
        switch name {
        case Names.listConversations:
            return listConversationsTool(args: args, store: store)
        case Names.searchConversations:
            return searchConversationsTool(args: args, store: store)
        default:
            return toolError("Unknown conversation tool: \(name)")
        }
    }

    // MARK: - list_conversations

    @MainActor
    private static func listConversationsTool(
        args: [String: Any],
        store: AppStateStore
    ) -> String {
        let source = (args["source"] as? String)?.trimmed.lowercased() ?? "all"
        let limit = clampedConversationLimit(args["limit"], default: 20, max: 50)

        var results: [[String: Any]] = []

        if source == "in_app" || source == "all" {
            let inAppRows = ChatHistoryStore.shared.conversations
                .prefix(limit)
                .map(serializeConversationSummary)
            results.append(contentsOf: inAppRows)
        }

        if source == "nostr" || source == "all" {
            let nostrRows = store.state.nostrConversations
                .sorted { $0.lastTouched > $1.lastTouched }
                .prefix(max(0, limit - results.count))
                .map { serializeNostrConversationSummary($0, store: store) }
            results.append(contentsOf: nostrRows)
        }

        return toolSuccess([
            "conversations": results,
            "count": results.count,
            "source": source,
        ])
    }

    // MARK: - search_conversations

    @MainActor
    private static func searchConversationsTool(
        args: [String: Any],
        store: AppStateStore
    ) -> String {
        guard let query = (args["query"] as? String)?.trimmed, !query.isEmpty else {
            return toolError("Missing or empty 'query'")
        }
        let source = (args["source"] as? String)?.trimmed.lowercased() ?? "all"
        let limit = clampedConversationLimit(args["limit"], default: 10, max: 25)
        let lowercasedQuery = query.lowercased()

        var hits: [[String: Any]] = []

        if source == "in_app" || source == "all" {
            for conversation in ChatHistoryStore.shared.conversations {
                for message in conversation.messages {
                    guard message.text.lowercased().contains(lowercasedQuery) else { continue }
                    hits.append(serializeInAppHit(message: message, conversation: conversation))
                    if hits.count >= limit { break }
                }
                if hits.count >= limit { break }
            }
        }

        if (source == "nostr" || source == "all"), hits.count < limit {
            let remaining = limit - hits.count
            var nostrHits = 0
            outer: for record in store.state.nostrConversations.sorted(by: { $0.lastTouched > $1.lastTouched }) {
                for turn in record.turns {
                    guard turn.content.lowercased().contains(lowercasedQuery) else { continue }
                    hits.append(serializeNostrHit(turn: turn, record: record, store: store))
                    nostrHits += 1
                    if nostrHits >= remaining { break outer }
                }
            }
        }

        return toolSuccess([
            "query": query,
            "total_found": hits.count,
            "results": hits,
            "source": source,
        ])
    }

    // MARK: - Serializers

    @MainActor
    private static func serializeConversationSummary(_ conversation: ChatConversation) -> [String: Any] {
        let userMessages = conversation.messages.filter {
            if case .user = $0.role { return true }
            return false
        }
        let assistantMessages = conversation.messages.filter {
            if case .assistant = $0.role { return true }
            return false
        }
        var row: [String: Any] = [
            "source": "in_app",
            "conversation_id": conversation.id.uuidString,
            "updated_at": iso8601Basic.string(from: conversation.updatedAt),
            "message_count": conversation.messages.count,
            "user_message_count": userMessages.count,
            "assistant_message_count": assistantMessages.count,
        ]
        let title = conversation.title.trimmingCharacters(in: .whitespacesAndNewlines)
        if !title.isEmpty {
            row["title"] = title
        } else {
            row["title"] = String(conversation.firstUserSnippet.prefix(80))
        }
        if let first = userMessages.first {
            row["first_user_message"] = String(first.text.prefix(200))
        }
        return row
    }

    @MainActor
    private static func serializeNostrConversationSummary(
        _ record: NostrConversationRecord,
        store: AppStateStore
    ) -> [String: Any] {
        let displayName = store.friend(identifier: record.counterpartyPubkey)?.displayName
            ?? NostrNpub.shortNpub(fromHex: record.counterpartyPubkey)
        var row: [String: Any] = [
            "source": "nostr",
            "root_event_id": record.rootEventID,
            "counterparty": displayName,
            "counterparty_pubkey": record.counterpartyPubkey,
            "first_seen": iso8601Basic.string(from: record.firstSeen),
            "last_touched": iso8601Basic.string(from: record.lastTouched),
            "turn_count": record.turns.count,
        ]
        if let first = record.turns.first {
            row["first_message"] = String(first.content.prefix(200))
        }
        return row
    }

    @MainActor
    private static func serializeInAppHit(
        message: ChatMessage,
        conversation: ChatConversation
    ) -> [String: Any] {
        let roleString: String
        switch message.role {
        case .user: roleString = "user"
        case .assistant: roleString = "assistant"
        default: roleString = "other"
        }
        let title = conversation.title.trimmingCharacters(in: .whitespacesAndNewlines)
        return [
            "source": "in_app",
            "conversation_id": conversation.id.uuidString,
            "conversation_title": title.isEmpty ? String(conversation.firstUserSnippet.prefix(80)) : title,
            "conversation_updated_at": iso8601Basic.string(from: conversation.updatedAt),
            "role": roleString,
            "timestamp": iso8601Basic.string(from: message.timestamp),
            "snippet": String(message.text.prefix(400)),
        ]
    }

    @MainActor
    private static func serializeNostrHit(
        turn: NostrConversationTurn,
        record: NostrConversationRecord,
        store: AppStateStore
    ) -> [String: Any] {
        let displayName = store.friend(identifier: record.counterpartyPubkey)?.displayName
            ?? NostrNpub.shortNpub(fromHex: record.counterpartyPubkey)
        return [
            "source": "nostr",
            "root_event_id": record.rootEventID,
            "counterparty": displayName,
            "direction": turn.direction.rawValue,
            "timestamp": iso8601Basic.string(from: turn.createdAt),
            "snippet": String(turn.content.prefix(400)),
        ]
    }

    // MARK: - Helpers

    private static func clampedConversationLimit(_ raw: Any?, default defaultValue: Int, max: Int) -> Int {
        guard let n = numericArg(raw) else { return defaultValue }
        return Swift.max(1, Swift.min(Int(n), max))
    }
}
