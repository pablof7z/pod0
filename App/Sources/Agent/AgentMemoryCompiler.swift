import Foundation
import os.log

/// Folds the active `AgentMemory` set into a single short
/// `CompiledAgentMemory.text` paragraph via an LLM call. Idempotent:
/// short-circuits when the current active-memory id sequence (oldest
/// first) already matches `state.compiledMemory.sourceMemoryIDs`, so
/// calling it after every agent run is cheap when no new memory was
/// recorded.
///
/// Most-recent memory wins on conflicts. The previous compiled paragraph
/// is included as a *reference* in the prompt so the consolidated voice
/// stays stable across compiles, but raw memories remain the source of
/// truth — the prompt makes that explicit.
@MainActor
struct AgentMemoryCompiler {

    private static let logger = Logger.app("AgentMemoryCompiler")

    let store: AppStateStore

    func compileIfNeeded() async {
        let memories = store.activeMemories
            .sorted { $0.createdAt < $1.createdAt }
        let previous = store.state.compiledMemory

        if memories.isEmpty {
            if previous != nil { store.setCompiledMemory(nil) }
            return
        }

        let currentIDs = memories.map(\.id)
        if previous?.sourceMemoryIDs == currentIDs { return }

        let model = store.state.settings.agentInitialModel
        let reference = LLMModelReference(storedID: model)
        guard !reference.isEmpty,
              LLMProviderCredentialResolver.hasAPIKey(for: reference.provider) else {
            // No usable LLM credential yet — leave the previous compile in
            // place and try again after the next run. This is the same
            // policy as a failed LLM call: never blow away a good compile.
            return
        }

        let userMessage = buildUserMessage(memories: memories, previous: previous)
        let messages: [[String: Any]] = [
            ["role": "system", "content": Self.systemPrompt],
            ["role": "user", "content": userMessage],
        ]

        let result: AgentResult
        do {
            result = try await AgentLLMClient.streamCompletion(
                messages: messages,
                tools: [],
                model: model,
                feature: CostFeature.agentChat,
                onPartialContent: { _ in }
            )
        } catch {
            Self.logger.error("compile chat call failed: \(error.localizedDescription, privacy: .public). Keeping previous compiled memory.")
            return
        }

        let raw = (result.assistantMessage["content"] as? String) ?? ""
        let trimmed = raw
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .trimmingCharacters(in: CharacterSet(charactersIn: "`"))
            .trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            Self.logger.notice("LLM returned empty compiled text. Keeping previous compiled memory.")
            return
        }

        store.setCompiledMemory(
            CompiledAgentMemory(
                text: trimmed,
                compiledAt: Date(),
                sourceMemoryCount: memories.count,
                sourceMemoryIDs: currentIDs
            )
        )
    }

    private static let systemPrompt: String = """
    You maintain a short paragraph summarizing what is known about a single user, used as durable context for an assistant. You will be given the full list of raw memories (each with an id and timestamp, oldest first), plus the previous compiled paragraph for reference.

    Produce a fresh consolidated summary as a single short paragraph (under 200 words, plain prose, no bullet lists, no headings, no preamble). Rules:
    - The raw memories are the sole source of truth. Derive everything from them.
    - When raw memories conflict, the most recent (latest timestamp) wins. Drop superseded facts.
    - Do not invent facts. Only state what the raw memories support.
    - Keep it concrete and short. Skip filler like "the user has shared various preferences".
    - Refer to the user in the third person ("they", "the user").

    Return only the paragraph itself. No JSON, no quotes, no markdown.
    """

    private func buildUserMessage(memories: [AgentMemory], previous: CompiledAgentMemory?) -> String {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime]

        let memoryLines = memories.map { memory in
            "- [\(formatter.string(from: memory.createdAt))] \(memory.id.uuidString): \(memory.content)"
        }

        var sections: [String] = []
        if let previous, !previous.text.isEmpty {
            sections.append("PREVIOUS COMPILED PARAGRAPH (reference only — raw memories override):")
            sections.append(previous.text)
            sections.append("")
        }
        sections.append("RAW MEMORIES (oldest first):")
        sections.append(memoryLines.joined(separator: "\n"))
        return sections.joined(separator: "\n")
    }
}
