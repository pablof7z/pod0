import Foundation
import Pod0Core

extension RootView {
    var hasUnreadAgentMessages: Bool {
        guard !showAgentChat, let session = agentSession else { return false }
        return session.messages.count > agentUnseenMessageCount
    }

    func openAgentChat(conversationID: ConversationId? = nil) {
        if agentSession == nil {
            agentSession = store.sharedLibrary?.makeAgentConversationSession()
        }
        guard agentSession != nil else { return }
        requestedAgentConversationID = conversationID
        showAgentChat = true
    }
}
