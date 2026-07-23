import SwiftUI

/// Top-level "Ask" tab — hosts the AI agent chat as a full-screen surface.
/// Wraps `AgentChatView` (formerly presented as a sheet) so it lives inline
/// in the tab bar.
struct AskAgentView: View {
    @Environment(AppStateStore.self) private var store
    @State private var session: SharedAgentConversationSession?

    var body: some View {
        if let session {
            SharedAgentChatView(session: session, requestedLegacyConversationID: nil)
        } else {
            Color.clear
                .onAppear {
                    session = store.sharedLibrary?.makeAgentConversationSession()
                }
        }
    }
}
