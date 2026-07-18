import Foundation

extension AgentChatSession {
    func startRecall(_ text: String) {
        let trimmed = text.trimmed
        guard RecallFeature.isEnabled, !trimmed.isEmpty, canSend else { return }
        sendingTask = Task { await sendRecall(trimmed) }
    }

    func sendRecall(_ text: String) async {
        let conversationID = currentConversationID
        if rawMessages.isEmpty {
            rawMessages.append([
                "role": "system",
                "content": AgentPrompt.build(for: store.state),
            ])
            seedRawMessagesFromHistory()
        }
        rawMessageCountAtLastSendStart = rawMessages.count
        messageCountAtLastSendStart = messages.count
        lastFailedMessage = nil
        rawMessages.append(["role": "user", "content": text])
        messages.append(ChatMessage(role: .user, text: text))
        phase = .sending
        persistCurrentConversation()

        let answer: RecallAnswer
        if let rag = podcastDeps?.rag {
            answer = await RecallAnswerService(rag: rag, store: store).answer(query: text)
        } else {
            answer = RecallAnswer(
                text: "Transcript recall is unavailable right now. Try again after reopening Pod0.",
                status: .unavailable
            )
        }

        guard !Task.isCancelled, currentConversationID == conversationID else {
            if currentConversationID == conversationID {
                phase = .idle
                persistCurrentConversation()
            }
            return
        }
        guard answer.status != .cancelled else {
            phase = .idle
            persistCurrentConversation()
            return
        }

        rawMessages.append(["role": "assistant", "content": answer.text])
        messages.append(ChatMessage(
            role: .assistant,
            text: answer.text,
            recallAnswer: answer
        ))
        phase = .idle
        persistCurrentConversation()
        store.recordProductSignal(.init(name: .agentTurnCompleted, outcome: .succeeded))
        maybeGenerateTitle()
    }
}
