import Foundation

// MARK: - ConversationHistorySkill
//
// Defines the `conversation_history` skill. When activated via
// `use_skill(skill_id: "conversation_history")` the agent receives the manual
// below and gains access to two tools: `list_conversations` and
// `search_conversations`. Both tools cover in-app chat threads and Nostr
// peer conversations in a unified interface.
//
// Search is lexical (case-insensitive substring) rather than embedding-based.
// The corpus is small enough that full-text scan is fast and avoids the cost
// of a separate vector index for conversation history.

enum ConversationHistorySkill {

    static let skill = AgentSkill(
        id: AgentSkillID.conversationHistory,
        displayName: "Conversation History",
        summary: "List and search through past in-app chat threads and Nostr peer conversations. Use when the user wants to recall or find something discussed in a prior session.",
        manual: manualText,
        toolNames: [
            AgentTools.Names.listConversations,
            AgentTools.Names.searchConversations,
        ],
        schema: { schemaEntries }
    )

    // MARK: - Manual

    private static let manualText: String = """
    # Conversation History Skill

    This skill surfaces the agent's own conversation history across two sources:

    - **In-app**: Chat threads started by the owner in the chat sheet (stored
      in `ChatHistoryStore`, up to 50 threads × 100 messages each).
    - **Nostr**: Threads the agent has participated in with Nostr peers
      (stored in `AppState.nostrConversations`).

    ## When to use this

    - The user says "what did we talk about last week?", "find the conversation
      where we discussed X", "did I ask you about Y before?", or similar.
    - The user wants to pick up a prior thread or recall a decision made in a
      previous session.

    Do NOT load this skill to answer questions about *podcast content* — those
    belong to `query_transcripts` and `search_episodes` (always-on tools).

    ## Tools

    - `list_conversations(source?, limit?)` — returns summaries of recent
      conversations: title (or first-message snippet), date, source tag,
      and message counts. Use this first to orient before searching.
    - `search_conversations(query, source?, limit?)` — full-text search
      through message content across all threads. Returns matching message
      snippets with their conversation context. Use when the user names a
      specific topic, phrase, or keyword from a prior session.

    ## Parameters

    `source` (both tools):
    - `"all"` (default) — searches both in-app and Nostr threads.
    - `"in_app"` — only in-app chat threads.
    - `"nostr"` — only Nostr peer conversations.

    `limit`:
    - `list_conversations`: 1–50, default 20.
    - `search_conversations`: 1–25, default 10.

    ## Suggested flow

    1. If the user's question is vague ("what did we talk about recently?"),
       call `list_conversations` to show titles and dates.
    2. If the user names a topic or phrase, call `search_conversations` with
       that phrase as `query` to find relevant message snippets.
    3. Surface the `conversation_id` (in-app) or `root_event_id` (Nostr) if
       the user wants to continue a thread — these are the handles they'd
       need to resume context.

    ## Privacy note

    This skill exposes the owner's full conversation history. It should only
    be activated in owner-initiated sessions, not on behalf of a Nostr peer
    unless the owner has explicitly granted that peer full access.
    """

    // MARK: - Tool schemas

    @MainActor
    private static var schemaEntries: [[String: Any]] {
        [listConversationsSchema, searchConversationsSchema]
    }

    // MARK: - list_conversations

    private static var listConversationsSchema: [String: Any] {
        functionTool(
            name: AgentTools.Names.listConversations,
            description: """
            List recent in-app chat threads and/or Nostr peer conversations. \
            Returns title (or first-message snippet), date, source, and message counts. \
            Use when the user asks what you've discussed before, or to orient before \
            a targeted search. Requires the conversation_history skill.
            """,
            properties: [
                "source": [
                    "type": "string",
                    "enum": ["all", "in_app", "nostr"],
                    "description": "Which conversation store to list. 'all' (default) covers both in-app threads and Nostr peer conversations.",
                ],
                "limit": [
                    "type": "integer",
                    "description": "Maximum conversations to return (1–50). Defaults to 20.",
                ],
            ],
            required: []
        )
    }

    // MARK: - search_conversations

    private static var searchConversationsSchema: [String: Any] {
        functionTool(
            name: AgentTools.Names.searchConversations,
            description: """
            Full-text search through past conversation messages (in-app and/or Nostr). \
            Returns matching message snippets with conversation context (title, date, source). \
            Use when the user mentions a specific topic, phrase, or keyword from a prior session. \
            Requires the conversation_history skill.
            """,
            properties: [
                "query": [
                    "type": "string",
                    "description": "Case-insensitive keyword or phrase to search for in message text.",
                ],
                "source": [
                    "type": "string",
                    "enum": ["all", "in_app", "nostr"],
                    "description": "Which conversation store to search. 'all' (default) covers both in-app threads and Nostr peer conversations.",
                ],
                "limit": [
                    "type": "integer",
                    "description": "Maximum matching messages to return (1–25). Defaults to 10.",
                ],
            ],
            required: ["query"]
        )
    }

    // MARK: - Helper

    private static func functionTool(
        name: String,
        description: String,
        properties: [String: Any],
        required: [String]
    ) -> [String: Any] {
        [
            "type": "function",
            "function": [
                "name": name,
                "description": description,
                "parameters": [
                    "type": "object",
                    "properties": properties,
                    "required": required,
                ] as [String: Any],
            ] as [String: Any],
        ]
    }
}
