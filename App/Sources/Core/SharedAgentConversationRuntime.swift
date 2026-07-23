import Pod0Core

@MainActor
protocol SharedAgentConversationRuntime: AnyObject {
    func execute(_ command: ApplicationCommand) async throws -> OperationResult?
    func subscribeAgentConversation(
        _ conversationID: ConversationId,
        subscriber: any ProjectionSubscriber
    ) -> SubscriptionId
    func unsubscribeAgentConversation(_ subscriptionID: SubscriptionId)
    func executePendingHostRequests()
}

extension SharedLibraryClient: SharedAgentConversationRuntime {
    func subscribeAgentConversation(
        _ conversationID: ConversationId,
        subscriber: any ProjectionSubscriber
    ) -> SubscriptionId {
        facade.subscribe(
            request: ProjectionRequest(
                scope: .agentConversation(conversationId: conversationID),
                offset: 0,
                maxItems: 64
            ),
            subscriber: subscriber
        )
    }

    func unsubscribeAgentConversation(_ subscriptionID: SubscriptionId) {
        facade.unsubscribe(subscriptionId: subscriptionID)
    }
}
