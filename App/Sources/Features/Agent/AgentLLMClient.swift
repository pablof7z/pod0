import Foundation

/// Provider-blind LLM client for agent-chat turns. All provider and credential
/// details are handled inside the Rust kernel via `nmp_app_podcast_chat_complete`.
/// Swift never sees OpenRouter vs. Ollama — it only sends a message array and
/// receives an assistant text string.
///
/// The `streamCompletion` interface is kept for call-site compatibility. Because
/// Rust returns a complete string rather than an SSE stream, `onPartialContent`
/// is called once with the full text when the reply arrives (the streaming UI
/// shows a "thinking" state for the duration of the Rust call, then snaps to
/// the full reply).
enum AgentLLMClient {

    /// Canned reply returned by the UI-test stub. Deterministic and unique so
    /// tests can assert on it without relying on network or a real LLM.
    static let uitestStubReply = "UITestStubReply: agent reply path is working."

    /// Run one LLM turn through the Rust provider-blind backend.
    ///
    /// - Parameters:
    ///   - messages: Full OpenAI-format message array (system + history + user).
    ///   - tools: Tool schema — passed through for call-site compatibility but
    ///     ignored in this path; Swift's turn-loop dispatches tools as before.
    ///   - model: Ignored — Rust selects from its stored configuration
    ///     (THINKING_MODEL with FAST_MODEL fallback). Kept for call-site compat.
    ///   - feature: Cost-ledger tag — ignored in the Rust path; kept for compat.
    ///   - ollamaChatURL: Ignored — Rust reads the URL from the kernel store.
    ///   - onPartialContent: Called once with the full reply text when it lands.
    /// - Returns: An `AgentResult` with the assistant message and empty
    ///   `toolCalls` (Rust does not run Swift tool dispatch).
    /// - Throws: `AgentError.httpError` on Rust error or unavailable handle.
    ///   `CancellationError` when the enclosing Task is cancelled.
    @MainActor
    static func streamCompletion(
        messages: [[String: Any]],
        tools: [[String: Any]],
        model: String,
        feature: String = CostFeature.agentChat,
        ollamaChatURL: URL? = nil,
        onPartialContent: (String) -> Void
    ) async throws -> AgentResult {
        try Task.checkCancellation()

        // UI-test stub: when the app is launched with --UITestAgentStub the
        // entire Rust FFI call is bypassed and a deterministic canned reply is
        // returned instead. The Swift turn-loop in AgentChatSession+Turns still
        // runs authentically (message appending, phase transitions, transcript
        // rendering) — only the provider-execution boundary is stubbed. This
        // lets simulator UI tests prove a reply is produced and rendered without
        // any network access or real LLM provider. Production builds are
        // unaffected because the guard fires only on the test-injected arg.
        if CommandLine.arguments.contains("--UITestAgentStub") {
            onPartialContent(uitestStubReply)
            let assistantMessage: [String: Any] = [
                "role": "assistant",
                "content": uitestStubReply,
            ]
            return AgentResult(assistantMessage: assistantMessage, toolCalls: [], tokensUsed: nil)
        }

        // Capture the opaque handle pointer on @MainActor before leaving.
        // The pointer is stable for the process lifetime (registered once in
        // PodcastHandle.init) — safe to hand to a detached background task.
        guard let handle = KernelModel.shared?.podcastHandlePointer else {
            throw AgentError.httpError("Kernel handle unavailable")
        }

        let messagesData = try JSONSerialization.data(withJSONObject: messages)
        guard let messagesJSON = String(data: messagesData, encoding: .utf8) else {
            throw AgentError.httpError("Failed to encode messages to JSON")
        }

        // Run the blocking Rust call off @MainActor so we don't stall the UI.
        // The handle pointer is safe to use from any thread (PodcastHandle
        // is marked Send+Sync; the FFI function is D6-safe and re-entrant).
        // Bridge the non-Sendable pointer across the task boundary as an Int.
        let handleBits = Int(bitPattern: handle)
        let responseJSON: String = await Task.detached(priority: .userInitiated) {
            guard let handle = UnsafeMutableRawPointer(bitPattern: handleBits) else {
                return #"{"error":"null kernel handle"}"#
            }
            return messagesJSON.withCString { msgPtr in
                guard let ptr = nmp_app_podcast_chat_complete(handle, msgPtr) else {
                    return #"{"error":"null response from Rust"}"#
                }
                defer { nmp_free_string(ptr) }
                return String(cString: ptr)
            }
        }.value

        try Task.checkCancellation()

        // Parse `{"text":"…"}` or `{"error":"…"}`.
        guard let responseData = responseJSON.data(using: .utf8),
              let responseObj = try? JSONSerialization.jsonObject(with: responseData) as? [String: Any]
        else {
            throw AgentError.malformedResponse
        }

        if let errorMsg = responseObj["error"] as? String {
            throw AgentError.httpError(errorMsg)
        }

        guard let text = responseObj["text"] as? String else {
            throw AgentError.malformedResponse
        }

        // Notify the streaming observer with the full reply (replaces SSE stream).
        if !text.isEmpty {
            onPartialContent(text)
        }

        let assistantMessage: [String: Any] = ["role": "assistant", "content": text]
        return AgentResult(
            assistantMessage: assistantMessage,
            toolCalls: [],
            tokensUsed: nil
        )
    }
}
