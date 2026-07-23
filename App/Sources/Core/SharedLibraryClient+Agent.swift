import Foundation

extension SharedLibraryClient {
    func attachAgent(
        approvalPresenter: any CoreAgentApprovalPresenting,
        playback: PlaybackState,
        store: AppStateStore
    ) {
        deferredAgentHost.attach(CoreAgentHost(
            capabilityExecutor: LiveCoreAgentCapabilityExecutor(engine: playback.engine),
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
}
