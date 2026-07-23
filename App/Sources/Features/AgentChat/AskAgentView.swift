import SwiftUI

/// Top-level "Ask" tab — hosts the AI agent chat as a full-screen surface.
/// The native surface renders typed projections from the shared Rust core.
struct AskAgentView: View {
    @Environment(AppStateStore.self) private var store
    @State private var session: SharedAgentConversationSession?

    var body: some View {
        if let session {
            SharedAgentChatView(session: session, requestedConversationID: nil)
        } else {
            Color.clear
                .onAppear {
                    session = store.sharedLibrary?.makeAgentConversationSession()
                }
        }
    }
}
