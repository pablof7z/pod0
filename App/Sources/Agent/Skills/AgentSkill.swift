import Foundation

// MARK: - AgentSkill
//
// A named bundle of (a) instructions the agent reads after opting in via
// `use_skill` and (b) tool schemas that become available for the rest of the
// conversation. Skills are listed by id + summary in the system prompt so the
// agent knows what exists without paying for full manuals every turn.
//
// Skill state lives on the session (`AgentChatSession.enabledSkills`) and is
// persisted into `ChatConversation` so auto-resume preserves it.

struct AgentSkill: Sendable {
    /// Stable identifier the agent passes to `use_skill(skill_id:)`.
    let id: String
    /// Human-readable name (not currently shown anywhere user-facing —
    /// reserved for a future Settings UI).
    let displayName: String
    /// One-line description rendered in the system prompt's `## Skills`
    /// catalog. Keep short — every conversation pays for these tokens.
    let summary: String
    /// Full instructions returned as the `manual` field of the `use_skill`
    /// tool result. Only consumed by the LLM after activation.
    let manual: String
    /// Tool-name constants this skill unlocks. Used by
    /// `AgentSkillRegistry.owningSkillID(forTool:)` for the dispatcher's
    /// defensive gate.
    let toolNames: [String]
    /// OpenAI-compatible tool schemas. Appended to the per-turn tool list when
    /// this skill's id is in `enabledSkills`. A closure so the schema
    /// construction stays @MainActor (matches `AgentTools.schema`).
    let schema: @MainActor @Sendable () -> [[String: Any]]
}

// MARK: - Canonical skill IDs

enum AgentSkillID {
    static let podcastGeneration = "podcast_generation"
    static let wikiResearch = "wiki_research"
    static let conversationHistory = "conversation_history"
    static let youtubeIngestion = "youtube_ingestion"
}
