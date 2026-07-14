import Foundation
import os.log

/// Defines the tools the agent can call and dispatches them to AppStateStore.
/// Add new tools by:
///   1. Adding a schema entry in `AgentToolSchema.swift`
///   2. Adding a case to `dispatch` below
///   3. Implementing the handler in the appropriate `AgentTools+*.swift` extension
enum AgentTools {

    static let logger = Logger.app("AgentTools")

    // MARK: - Constants

    /// Maximum number of characters used when truncating text in activity summaries.
    static let summaryTruncationLength = 40

    // MARK: - Tool names

    /// Canonical string identifiers for every tool the agent can call.
    enum Names {
        static let createNote        = "create_note"
        static let recordMemory      = "record_memory"
        /// Sentinel name for the in-band model upgrade. Handled directly by
        /// the turn loop (AgentChatSession / AgentRelayBridge) — NOT routed
        /// through `AgentTools.dispatch`, because the "side effect" is a
        /// session-local flag, not anything in `AppStateStore`.
        static let upgradeThinking   = "upgrade_thinking"
        /// Sentinel name for the in-band skill activation. Like
        /// `upgradeThinking`, handled directly by the turn loop — the
        /// "side effect" is a session-local `enabledSkills` insert, not a
        /// store mutation. See `AgentSkillRegistry`.
        static let useSkill          = "use_skill"
        /// Pauses the agent loop to consult the owner via a modal sheet.
        /// Routed through the normal `dispatch` switch (it's a regular
        /// tool that returns content to the LLM), but requires an
        /// `AgentAskCoordinator` to be wired — when absent, the
        /// dispatcher returns a typed error envelope rather than
        /// crashing.
        static let ask                  = "ask"
        static let scheduleTask         = "schedule_task"
        static let cancelScheduledTask  = "cancel_scheduled_task"
        static let listScheduledTasks   = "list_scheduled_tasks"

        // Skill-gated: requires the `conversation_history` skill.
        static let listConversations    = "list_conversations"
        static let searchConversations  = "search_conversations"
    }

    // MARK: - Cached formatters

    nonisolated(unsafe) static let iso8601WithFractional: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return f
    }()

    nonisolated(unsafe) static let iso8601Basic = ISO8601DateFormatter()

    // MARK: - Dispatcher

    /// Routes a tool call to the right handler.
    ///
    /// When `podcastDeps` is supplied, podcast-domain tool names
    /// (`AgentTools.PodcastNames.all`) are routed through `dispatchPodcast`.
    /// When it's `nil`, podcast tools surface a clear "not configured" error
    /// envelope so the agent loop continues without crashing.
    @MainActor
    static func dispatch(
        name: String,
        argsJSON: String,
        store: AppStateStore,
        batchID: UUID,
        podcastDeps: PodcastAgentToolDeps? = nil,
        enabledSkills: Set<String> = [],
        askCoordinator: AgentAskCoordinator? = nil
    ) async -> String {
        let args: [String: Any]
        do {
            args = try JSONSerialization.jsonObject(with: Data(argsJSON.utf8)) as? [String: Any] ?? [:]
        } catch {
            logger.error("AgentTools: failed to parse argsJSON for tool '\(name, privacy: .public)': \(error.localizedDescription, privacy: .public)")
            args = [:]
        }

        switch name {
        case Names.createNote, Names.recordMemory:
            return dispatchNotesMemory(name: name, args: args, store: store, batchID: batchID)

        case Names.scheduleTask, Names.cancelScheduledTask, Names.listScheduledTasks:
            return dispatchSchedule(name: name, args: args, store: store)

        case Names.listConversations, Names.searchConversations:
            guard enabledSkills.contains(AgentSkillID.conversationHistory) else {
                return toolError("Tool '\(name)' requires the 'conversation_history' skill — call use_skill(skill_id: \"conversation_history\") first.")
            }
            return await dispatchConversations(name: name, args: args, store: store)

        case Names.ask:
            // Pull the primitives off the non-Sendable `[String: Any]`
            // synchronously so nothing crosses the upcoming `await` —
            // same pattern `dispatchPodcast` uses for its routing call.
            let askQuestion = (args["question"] as? String) ?? ""
            let askContext = args["context"] as? String
            return await askOwnerTool(
                question: askQuestion,
                context: askContext,
                coordinator: askCoordinator
            )

        default:
            if PodcastNames.all.contains(name) {
                guard let podcastDeps else {
                    logger.error("AgentTools: podcast tool '\(name, privacy: .public)' invoked without PodcastAgentToolDeps — caller forgot to wire LivePodcastAgentToolDeps")
                    return toolError("Podcast tools are not wired up in this session.")
                }
                logger.info("AgentTools: dispatching podcast tool '\(name, privacy: .public)'")
                // Re-route through the JSON-string entrypoint so the
                // `[String: Any]` payload (non-Sendable) never crosses the
                // dispatch boundary — `dispatchPodcast(argsJSON:)` reparses on
                // the destination side.
                return await dispatchPodcast(
                    name: name,
                    argsJSON: argsJSON,
                    deps: podcastDeps,
                    enabledSkills: enabledSkills
                )
            }
            return toolError("Unknown tool: \(name)")
        }
    }

    // MARK: - Helpers

    /// Builds a JSON success response, merging `payload` into `{"success": true}`.
    static func toolSuccess(_ payload: [String: Any] = [:]) -> String {
        var result: [String: Any] = ["success": true]
        result.merge(payload) { _, new in new }
        do {
            return try String(data: JSONSerialization.data(withJSONObject: result), encoding: .utf8) ?? "{\"success\":true}"
        } catch {
            logger.error("AgentTools: failed to serialize success payload: \(error.localizedDescription, privacy: .public)")
            return "{\"success\":true}"
        }
    }

    /// Builds a JSON error response with the given message.
    static func toolError(_ message: String) -> String {
        let payload: [String: Any] = ["error": message]
        do {
            return try String(data: JSONSerialization.data(withJSONObject: payload), encoding: .utf8) ?? "{\"error\":\"unknown\"}"
        } catch {
            logger.error("AgentTools: failed to serialize error payload '\(message, privacy: .public)': \(error.localizedDescription, privacy: .public)")
            return "{\"error\":\"unknown\"}"
        }
    }

    /// Returns `s` truncated to `summaryTruncationLength` characters with a
    /// trailing ellipsis when the string was shortened, or the original string
    /// when it fits within the limit.
    static func truncated(_ s: String) -> String {
        s.count > summaryTruncationLength
            ? "\(s.prefix(summaryTruncationLength))…"
            : s
    }
}
