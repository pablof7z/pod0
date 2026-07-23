import Pod0Core

@MainActor
protocol SharedAgentConversationRuntime: AnyObject {
    func execute(_ command: ApplicationCommand) async throws -> OperationResult?
    func agentConversationHistory() -> AgentConversationsProjection
    func subscribeAgentConversation(
        _ conversationID: ConversationId,
        subscriber: any ProjectionSubscriber
    ) -> SubscriptionId
    func unsubscribeAgentConversation(_ subscriptionID: SubscriptionId)
    func executePendingHostRequests()
}

extension SharedLibraryClient: SharedAgentConversationRuntime {
    func agentConversationHistory() -> AgentConversationsProjection {
        let pageSize: UInt32 = 100
        let maximumItems = 500
        var offset: UInt32 = 0
        var conversations: [AgentConversationSummaryProjection] = []
        var failure: CoreFailure?
        var hasMore = false
        repeat {
            let envelope = facade.snapshot(request: ProjectionRequest(
                scope: .agentConversations,
                offset: offset,
                maxItems: UInt16(pageSize)
            ))
            guard case .agentConversations(let projection) = envelope.projection else { break }
            conversations.append(contentsOf: projection.conversations)
            failure = failure ?? projection.failure
            hasMore = projection.hasMore
            guard hasMore, conversations.count < maximumItems,
                  offset <= UInt32.max - pageSize else { break }
            offset += pageSize
        } while true
        if conversations.count > maximumItems {
            conversations.removeLast(conversations.count - maximumItems)
            hasMore = true
        }
        return AgentConversationsProjection(
            conversations: conversations,
            hasMore: hasMore,
            failure: failure
        )
    }

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

extension SharedAgentConversationRuntime {
    func agentConversationHistory() -> AgentConversationsProjection {
        AgentConversationsProjection(conversations: [], hasMore: false, failure: nil)
    }
}
