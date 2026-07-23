import Foundation

extension SharedLibraryClient {
    func attachAgent(
        approvalPresenter: any CoreAgentApprovalPresenting,
        playback: PlaybackState,
        store: AppStateStore
    ) {
        deferredAgentHost.attach(CoreAgentHost(
            capabilityExecutor: LiveCoreAgentCapabilityExecutor(engine: playback.engine),
            streamingState: agentStreamingState,
            approvalPresenter: approvalPresenter,
            systemPrompt: { [weak store] in
                guard let store else { return "" }
                return AgentPrompt.build(for: store.state)
            },
            ollamaURL: { [weak store] in
                store.flatMap { URL(string: $0.state.settings.ollamaChatURL) }
            }
        ))
    }

    func makeAgentConversationSession() -> SharedAgentConversationSession {
        SharedAgentConversationSession(
            runtime: self,
            streamingState: agentStreamingState,
            resumeConversationID: AgentConversationPointerStore().load(),
            onConversationChanged: { conversationID in
                AgentConversationPointerStore().save(conversationID)
            },
            modelReference: { [weak store] in
                store?.state.settings.agentInitialModel ?? ""
            }
        )
    }
}
