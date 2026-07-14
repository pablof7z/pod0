import Foundation

// Multi-conversation surface for `AgentChatSession`. Kept in a separate file
// so the core class file stays small. Title generation lives here too because
// it's tightly coupled with conversation persistence (the trigger fires when
// the first assistant reply lands and we hold no title yet).

extension AgentChatSession {

    /// Persists the current conversation to disk. Called from every turn-loop
    /// exit point and after `regenerateLast` so the store always reflects the
    /// latest visible transcript.
    func persistCurrentConversation() {
        let existing = history.conversation(id: currentConversationID)
        let convo = ChatConversation(
            id: currentConversationID,
            title: existing?.title ?? "",
            messages: messages,
            isUpgraded: isUpgraded,
            enabledSkills: enabledSkills,
            isScheduledTask: isScheduledTask,
            createdAt: existing?.createdAt ?? Date(),
            updatedAt: Date()
        )
        history.upsert(convo)
    }

    /// Clears the current conversation only — other history threads stay put.
    /// Called from the trash-can confirmation in the chat toolbar.
    func clearHistory() async {
        await stopInFlightWork()
        history.delete(currentConversationID)
        resetSessionStateForNewConversation()
    }

    /// Starts a fresh conversation, persisting the current one first so it
    /// remains accessible from the history sheet.
    func startNewConversation() async {
        await stopInFlightWork()
        // The in-flight conversation, if any, is already on disk via the
        // turn-loop's per-step persist calls; nothing extra to flush here.
        resetSessionStateForNewConversation()
    }

    /// Switches the session to a previously persisted conversation. The user
    /// explicitly chose this thread, so `loadedFromHistory` stays `false` —
    /// the "Continuing from your previous session" banner only signals the
    /// time-based auto-resume.
    func switchToConversation(_ id: UUID) async {
        guard let convo = history.conversation(id: id) else { return }
        await stopInFlightWork()

        setCurrentConversationID(convo.id)
        messages = convo.messages
        rawMessages = []
        isUpgraded = convo.isUpgraded
        enabledSkills = convo.enabledSkills
        phase = .idle
        lastFailedMessage = nil
        streamingContent = nil
        currentToolName = nil
        rawMessageCountAtLastSendStart = 0
        messageCountAtLastSendStart = 0
        loadedFromHistory = false
    }

    /// Cancels and awaits any in-flight send + title-generation tasks so that
    /// subsequent state mutations don't race the still-running turn loop.
    /// Without this, a switch mid-stream would have the in-flight turn append
    /// its assistant reply into the *new* conversation's `messages` array.
    private func stopInFlightWork() async {
        let send = sendingTask
        let title = titleTask
        send?.cancel()
        title?.cancel()
        if let send { await send.value }
        if let title { await title.value }
        sendingTask = nil
        titleTask = nil
    }

    private func resetSessionStateForNewConversation() {
        setCurrentConversationID(UUID())
        messages = []
        rawMessages = []
        isUpgraded = false
        enabledSkills = []
        phase = .idle
        lastFailedMessage = nil
        streamingContent = nil
        currentToolName = nil
        rawMessageCountAtLastSendStart = 0
        messageCountAtLastSendStart = 0
        loadedFromHistory = false
    }

    // MARK: - Title generation

    /// Kicks off async title generation once the conversation has its first
    /// user+assistant exchange and no title yet. Safe to call repeatedly —
    /// guards keep it idempotent. Called from the turn-loop after appending
    /// each non-empty assistant text reply.
    ///
    /// Reads from the live `messages` array (not the persisted snapshot)
    /// because this fires *before* `persistCurrentConversation()` and would
    /// otherwise miss the assistant message that just landed.
    func maybeGenerateTitle() {
        guard titleTask == nil else { return }
        // If a title already exists for the persisted conversation, skip.
        if let existing = history.conversation(id: currentConversationID),
           !existing.title.isEmpty {
            return
        }
        let hasUser = messages.contains { if case .user = $0.role { return true } else { return false } }
        let hasAssistant = messages.contains { if case .assistant = $0.role { return true } else { return false } }
        guard hasUser, hasAssistant else { return }

        let conversationID = currentConversationID
        let model = store.state.settings.memoryCompilationModel
        let snippet = AgentChatTitleGenerator.buildTranscriptSnippet(from: messages)
        let store = self.history
        titleTask = Task { [weak self] in
            let title = await AgentChatTitleGenerator.generate(transcript: snippet, model: model)
            await MainActor.run {
                if let title, !title.isEmpty {
                    store.setTitle(title, for: conversationID)
                }
                self?.titleTask = nil
            }
        }
    }
}
