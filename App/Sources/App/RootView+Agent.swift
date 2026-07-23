import Foundation

extension RootView {
    var hasUnreadAgentMessages: Bool {
        guard !showAgentChat, let session = agentSession else { return false }
        return session.messages.count > agentUnseenMessageCount
    }

    func openAgentChat(legacyConversationID: UUID? = nil) {
        if agentSession == nil {
            agentSession = store.sharedLibrary?.makeAgentConversationSession()
        }
        guard agentSession != nil else { return }
        self.legacyAgentConversationID = legacyConversationID
        showAgentChat = true
    }
}
