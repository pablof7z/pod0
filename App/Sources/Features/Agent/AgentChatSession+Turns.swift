import Foundation

// Turn-loop, retry/regenerate, and small response-shaping helpers split out of
// `AgentChatSession` so the core class stays under the 500-line file limit.
// Members touched here are intentionally module-internal on the parent type;
// extension files cannot see `private` members.

extension AgentChatSession {

    func retry() {
        guard let msg = lastFailedMessage, canSend else { return }
        rawMessages = Array(rawMessages.prefix(rawMessageCountAtLastSendStart))
        messages = Array(messages.prefix(messageCountAtLastSendStart))
        lastFailedMessage = nil
        sendingTask = Task { await send(msg, source: .typedChat) }
    }

    /// Regenerates the last assistant response by dropping it from the
    /// transcript and re-running the preceding user turn.
    ///
    /// Only valid when the last two visible messages are `.user` then `.assistant`
    /// (i.e. a pure text turn with no tool calls). Tool-batch turns have their
    /// own undo flow via the activity chip and are excluded here.
    ///
    /// Idempotent guard: does nothing if `phase == .sending`.
    var canRegenerate: Bool {
        guard canSend else { return false }
        let visible = messages.filter {
            if case .toolBatch = $0.role { return false }
            if case .error = $0.role { return false }
            if case .skillActivated = $0.role { return false }
            return true
        }
        guard visible.count >= 2 else { return false }
        let last = visible[visible.count - 1]
        let prev = visible[visible.count - 2]
        guard case .assistant = last.role, case .user = prev.role else { return false }
        return true
    }

    func regenerateLast() {
        guard canRegenerate else { return }

        guard let lastAssistantIdx = messages.lastIndex(where: {
            if case .assistant = $0.role { return true }
            return false
        }) else { return }
        let userText: String
        guard let prevUserIdx = messages[..<lastAssistantIdx].lastIndex(where: {
            if case .user = $0.role { return true }
            return false
        }) else { return }
        userText = messages[prevUserIdx].text

        messages.remove(at: lastAssistantIdx)

        if let rawIdx = rawMessages.lastIndex(where: { ($0["role"] as? String) == "assistant" }) {
            rawMessages.remove(at: rawIdx)
        }

        rawMessageCountAtLastSendStart = rawMessages.count - 1
        messageCountAtLastSendStart = messages.count - 1
        lastFailedMessage = nil

        persistCurrentConversation()
        sendingTask = Task { await regenerateSend(userText, source: .typedChat) }
    }

    /// Like `send(_:)` but skips appending the user message — it's already in
    /// both `messages` and `rawMessages` from the original turn.
    func regenerateSend(_ text: String, source: AgentRunSource) async {
        guard selectedProviderHasCredential() else {
            phase = .failed(missingCredentialMessage())
            return
        }

        if !rawMessages.isEmpty {
            rawMessages[0] = ["role": "system", "content": AgentPrompt.build(for: store.state)]
        }

        rawMessageCountAtLastSendStart = rawMessages.count
        messageCountAtLastSendStart = messages.count
        lastFailedMessage = text
        phase = .sending
        persistCurrentConversation()

        await runAgentTurns(batchID: UUID(), source: source, initialInput: text)
    }

    /// Begins an agent turn in a stored `Task` so the caller can cancel it via
    /// `cancelSend()`. Returns immediately; observe `phase` for progress.
    func startSend(_ text: String, source: AgentRunSource = .typedChat) {
        let trimmed = text.trimmed
        guard !trimmed.isEmpty, canSend else { return }
        sendingTask = Task { await send(trimmed, source: source) }
    }

    func send(_ text: String, source: AgentRunSource) async {
        let trimmed = text.trimmed
        guard !trimmed.isEmpty else { return }

        guard selectedProviderHasCredential() else {
            phase = .failed(missingCredentialMessage())
            return
        }

        if rawMessages.isEmpty {
            rawMessages.append([
                "role": "system",
                "content": AgentPrompt.build(for: store.state),
            ])
            seedRawMessagesFromHistory()
        } else {
            rawMessages[0] = [
                "role": "system",
                "content": AgentPrompt.build(for: store.state),
            ]
        }

        rawMessageCountAtLastSendStart = rawMessages.count
        messageCountAtLastSendStart = messages.count
        lastFailedMessage = trimmed

        rawMessages.append(["role": "user", "content": trimmed])
        messages.append(ChatMessage(role: .user, text: trimmed))
        phase = .sending
        persistCurrentConversation()

        await runAgentTurns(batchID: UUID(), source: source, initialInput: trimmed)
    }

    /// Executes the streaming agent turn-loop, processing LLM responses and tool
    /// calls until the model produces a text-only reply, the task is cancelled, a
    /// network error occurs, or `maxTurns` is exhausted.
    func runAgentTurns(batchID: UUID, source: AgentRunSource, initialInput: String) async {
        var batchActionCount = 0
        let systemPromptSnapshot = (rawMessages.first?["content"] as? String) ?? ""
        let collector = AgentRunCollector(
            id: batchID,
            source: source,
            initialInput: initialInput,
            systemPrompt: systemPromptSnapshot
        )
        var turnNumber = 0

        for _ in 0..<maxTurns {
            streamingContent = ""

            let messagesBeforeCall = rawMessages
            let result: AgentResult
            let modelForTurn = isUpgraded
                ? store.state.settings.agentThinkingModel
                : store.state.settings.agentInitialModel
            do {
                let ollamaChatURL = URL(string: store.state.settings.ollamaChatURL)
                result = try await AgentLLMClient.streamCompletion(
                    messages: rawMessages,
                    tools: AgentTools.schema
                         + AgentTools.podcastSchema
                         + AgentSkillRegistry.schemas(for: enabledSkills),
                    model: modelForTurn,
                    ollamaChatURL: ollamaChatURL
                ) { [weak self] partial in
                    self?.streamingContent = partial
                }
            } catch is CancellationError {
                collector.appendTurn(
                    turnNumber: turnNumber,
                    messagesBeforeCall: messagesBeforeCall,
                    apiResponse: nil,
                    toolDispatches: []
                )
                collector.finish(outcome: .cancelled)
                resetStreamingState()
                lastFailedMessage = nil
                phase = .idle
                persistCurrentConversation()
                return
            } catch {
                preservePartialContentIfNeeded()
                collector.appendTurn(
                    turnNumber: turnNumber,
                    messagesBeforeCall: messagesBeforeCall,
                    apiResponse: nil,
                    toolDispatches: []
                )
                collector.finish(outcome: .failed, failureReason: error.localizedDescription)
                resetStreamingState()
                let msg = "Couldn't reach the agent. \(error.localizedDescription)"
                messages.append(ChatMessage(role: .error, text: msg))
                phase = .failed(msg)
                persistCurrentConversation()
                return
            }

            resetStreamingState()
            rawMessages.append(result.assistantMessage)

            if let content = result.assistantMessage["content"] as? String,
               !content.isBlank {
                messages.append(ChatMessage(role: .assistant, text: content))
                maybeGenerateTitle()
            }

            let apiResponse = makeAPIResponse(from: result)

            if result.toolCalls.isEmpty {
                collector.appendTurn(
                    turnNumber: turnNumber,
                    messagesBeforeCall: messagesBeforeCall,
                    apiResponse: apiResponse,
                    toolDispatches: []
                )
                collector.finish(outcome: .completed)
                lastFailedMessage = nil
                phase = .idle
                persistCurrentConversation()
                // Fire-and-forget memory compile. The compiler short-circuits
                // when active-memory ids match the previous compile's
                // `sourceMemoryIDs`, so this is a cheap no-op when the run
                // didn't touch `record_memory`. Never blocks the UI.
                let storeRef = store
                Task { @MainActor in
                    await AgentMemoryCompiler(store: storeRef).compileIfNeeded()
                }
                return
            }

            var toolDispatches: [AgentToolDispatch] = []
            for toolCall in result.toolCalls {
                guard !Task.isCancelled else {
                    collector.appendTurn(
                        turnNumber: turnNumber,
                        messagesBeforeCall: messagesBeforeCall,
                        apiResponse: apiResponse,
                        toolDispatches: toolDispatches
                    )
                    collector.finish(outcome: .cancelled)
                    resetStreamingState()
                    lastFailedMessage = nil
                    phase = .idle
                    persistCurrentConversation()
                    return
                }
                currentToolName = toolCall.name
                let activityCountBefore = store.state.agentActivity.count
                let resultJSON: String
                if toolCall.name == AgentTools.Names.upgradeThinking {
                    isUpgraded = true
                    resultJSON = AgentTools.toolSuccess([
                        "upgraded": true,
                        "model": store.state.settings.agentThinkingModel,
                    ])
                } else if toolCall.name == AgentTools.Names.useSkill {
                    resultJSON = handleUseSkill(argsJSON: toolCall.arguments)
                } else {
                    resultJSON = await AgentTools.dispatch(
                        name: toolCall.name,
                        argsJSON: toolCall.arguments,
                        store: store,
                        batchID: batchID,
                        podcastDeps: podcastDeps?.withChatConversationID(currentConversationID),
                        enabledSkills: enabledSkills,
                        askCoordinator: askCoordinator
                    )
                }
                rawMessages.append([
                    "role": "tool",
                    "tool_call_id": toolCall.id,
                    "content": resultJSON,
                ])
                batchActionCount += store.state.agentActivity.count - activityCountBefore
                toolDispatches.append(makeToolDispatch(call: toolCall, resultJSON: resultJSON))
            }
            collector.appendTurn(
                turnNumber: turnNumber,
                messagesBeforeCall: messagesBeforeCall,
                apiResponse: apiResponse,
                toolDispatches: toolDispatches
            )
            turnNumber += 1
            resetStreamingState()

            if batchActionCount > 0 {
                if let lastBatchIdx = messages.lastIndex(where: { msg in
                    if case .toolBatch(let id, _) = msg.role, id == batchID { return true }
                    return false
                }) {
                    messages[lastBatchIdx] = ChatMessage(
                        role: .toolBatch(batchID: batchID, count: batchActionCount),
                        text: ""
                    )
                } else {
                    messages.append(ChatMessage(
                        role: .toolBatch(batchID: batchID, count: batchActionCount),
                        text: ""
                    ))
                }
            }
            persistCurrentConversation()
        }

        collector.finish(outcome: .turnsExhausted)
        resetStreamingState()
        let limitMsg = "The agent reached its turn limit. Try a simpler request or start a new conversation."
        messages.append(ChatMessage(role: .error, text: limitMsg))
        phase = .failed(limitMsg)
        persistCurrentConversation()
    }

    func makeAPIResponse(from result: AgentResult) -> AgentAPIResponse {
        let runToolCalls: [AgentRunToolCall] = result.toolCalls.map { call in
            let parsedArgs = (try? JSONSerialization.jsonObject(with: Data(call.arguments.utf8)) as? [String: Any]) ?? [:]
            return AgentRunToolCall(id: call.id, name: call.name, arguments: parsedArgs)
        }
        let usage = result.tokensUsed ?? AgentTokenUsage(promptTokens: 0, completionTokens: 0, cachedTokens: nil)
        return AgentAPIResponse(
            assistantMessage: result.assistantMessage,
            toolCalls: runToolCalls,
            tokensUsed: usage
        )
    }

    func makeToolDispatch(call: AgentToolCall, resultJSON: String) -> AgentToolDispatch {
        let argsDict = (try? JSONSerialization.jsonObject(with: Data(call.arguments.utf8)) as? [String: Any]) ?? [:]
        let resultDict: [String: Any]
        if let parsed = try? JSONSerialization.jsonObject(with: Data(resultJSON.utf8)) as? [String: Any] {
            resultDict = parsed
        } else {
            resultDict = ["raw": resultJSON]
        }
        let errorMessage = resultDict["error"] as? String
        return AgentToolDispatch(
            toolCallID: call.id,
            toolName: call.name,
            arguments: argsDict,
            result: resultDict,
            error: errorMessage
        )
    }

    /// Clears transient streaming state that must be reset on every exit path.
    func resetStreamingState() {
        streamingContent = nil
        currentToolName = nil
    }

    /// In-band handler for the `use_skill` tool call. Mirrors the
    /// `upgrade_thinking` pattern — the "side effect" is a session-local
    /// `enabledSkills` insert, so we intercept rather than route through
    /// `AgentTools.dispatch`. The shared activation contract lives in
    /// `AgentSkillRegistry.activate(argsJSON:currentEnabledSkills:)`.
    func handleUseSkill(argsJSON: String) -> String {
        let result = AgentSkillRegistry.activate(
            argsJSON: argsJSON,
            currentEnabledSkills: enabledSkills
        )
        let newlyEnabled = result.updatedEnabledSkills.subtracting(enabledSkills)
        enabledSkills = result.updatedEnabledSkills
        for skillID in newlyEnabled {
            if let skill = AgentSkillRegistry.skill(id: skillID) {
                messages.append(ChatMessage(
                    role: .skillActivated(skillID: skill.id, displayName: skill.displayName),
                    text: ""
                ))
            }
        }
        return result.resultJSON
    }

    func selectedProviderHasCredential() -> Bool {
        let reference = LLMModelReference(storedID: store.state.settings.agentInitialModel)
        return LLMProviderCredentialResolver.hasAPIKey(for: reference.provider)
    }

    func missingCredentialMessage() -> String {
        let reference = LLMModelReference(storedID: store.state.settings.agentInitialModel)
        return LLMProviderCredentialResolver.missingCredentialMessage(for: reference.provider)
    }

    /// Saves any non-empty partial streaming content as an assistant message
    /// before an error terminates the turn.
    func preservePartialContentIfNeeded() {
        guard let partial = streamingContent,
              !partial.isBlank else { return }
        messages.append(ChatMessage(role: .assistant, text: partial))
    }

    func seedRawMessagesFromHistory() {
        for msg in messages {
            switch msg.role {
            case .user:
                rawMessages.append(["role": "user", "content": msg.text])
            case .assistant:
                rawMessages.append(["role": "assistant", "content": msg.text])
            case .toolBatch, .error, .skillActivated:
                continue
            }
        }
    }
}
